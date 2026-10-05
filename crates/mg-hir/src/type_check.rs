//! Static expression typing (spec §5.5, §5.8–§5.11), run over the HIR
//! after every declaration's names exist (`crate::lower`'s pass 1), so a
//! `let` or `glyphs.<name>` reference can point anywhere in the file
//! regardless of textual order (spec §5.11 rule 7).
//!
//! A `let`'s type is not known until its own initializer is typed, and
//! that initializer may reference another `let` — so this is a small
//! on-demand, memoized inference, not a simple table lookup. The
//! memoization doubles as the guard against infinite recursion on a
//! reference cycle: cycles are an *evaluation* error (spec §4.3), which is
//! M3's job, so a cycle hit here just resolves to [`Type::Error`] with no
//! diagnostic of its own, deferring to M3's real cycle report.

use indexmap::IndexSet;
use mg_diag::Diagnostic;
use mg_syntax::ast::{self, AstNode};
use mg_syntax::syntax_kind::SyntaxKind;

use crate::model::{GlyphKey, Hir};
use crate::resolve;
use crate::types::{self, Type};
use mg_diag::codes;

pub struct Ctx<'a> {
    pub hir: &'a mut Hir,
    pub current_glyph: Option<GlyphKey>,
    top_visiting: IndexSet<String>,
    glyph_visiting: IndexSet<String>,
    pub diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> Ctx<'a> {
    pub fn new(hir: &'a mut Hir, diagnostics: &'a mut Vec<Diagnostic>) -> Self {
        Self {
            hir,
            current_glyph: None,
            top_visiting: IndexSet::new(),
            glyph_visiting: IndexSet::new(),
            diagnostics,
        }
    }

    fn error(&mut self, diagnostic: Diagnostic) -> Type {
        self.diagnostics.push(diagnostic);
        Type::Error
    }
}

/// The type of `expr`, evaluated in `ctx`'s current scope. Every case that
/// cannot produce a sensible type reports exactly one diagnostic and
/// returns [`Type::Error`], so callers never need to re-check for `None`.
pub fn infer_expr(ctx: &mut Ctx, expr: &ast::Expr) -> Type {
    use ast::Expr;
    match expr {
        Expr::Literal(lit) => infer_literal(lit),
        Expr::Ident(ident) => infer_ident(ctx, ident),
        Expr::Paren(paren) => match paren.inner() {
            Some(inner) => infer_expr(ctx, &inner),
            None => Type::Error,
        },
        Expr::Tuple(tuple) => infer_tuple(ctx, tuple),
        Expr::List(list) => infer_list(ctx, list),
        Expr::Map(map) => {
            let span = map.syntax().text_range();
            ctx.error(Diagnostic::error(
                codes::FIELD_ILLEGAL_HERE,
                "a map literal is only legal as `caps` or `joinAt`",
                mg_diag::Label::new(span.into(), "not legal here"),
            ))
        }
        Expr::Unary(unary) => infer_unary(ctx, unary),
        Expr::Bin(bin) => infer_bin(ctx, bin),
        Expr::Call(call) => infer_call(ctx, call),
        Expr::Member(member) => infer_member(ctx, member),
        Expr::Range(_) => Type::Range,
        Expr::Error(_) => Type::Error,
    }
}

fn infer_literal(lit: &ast::Literal) -> Type {
    let Some(token) = lit.token() else {
        return Type::Error;
    };
    match token.kind() {
        SyntaxKind::NUMBER
        | SyntaxKind::NUMBER_ANGLE
        | SyntaxKind::NUMBER_RATIO
        | SyntaxKind::NUMBER_HEX
        | SyntaxKind::NUMBER_CODEPOINT
        | SyntaxKind::NUMBER_CHAR => Type::Num,
        SyntaxKind::STRING => Type::String,
        SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => Type::Bool,
        _ => Type::Error,
    }
}

fn glyph_scope_names(ctx: &Ctx) -> Vec<String> {
    let Some(key) = &ctx.current_glyph else {
        return Vec::new();
    };
    let Some(glyph) = ctx.hir.glyphs.get(key) else {
        return Vec::new();
    };
    glyph
        .lets
        .keys()
        .chain(glyph.anchors.keys())
        .chain(glyph.paths.iter().filter_map(|p| p.name.as_ref()))
        .cloned()
        .collect()
}

fn top_level_names(ctx: &Ctx) -> Vec<String> {
    ctx.hir
        .params
        .keys()
        .chain(ctx.hir.metrics.keys())
        .chain(ctx.hir.lets.keys())
        .cloned()
        .collect()
}

fn infer_ident(ctx: &mut Ctx, ident: &ast::IdentExpr) -> Type {
    let Some(token) = ident.token() else {
        return Type::Error;
    };
    let name = token.text().to_string();
    let span = token.text_range();

    if let Some(key) = ctx.current_glyph.clone()
        && let Some(ty) = glyph_local_type(ctx, &key, &name)
    {
        return ty;
    }

    if let Some(ty) = top_level_type(ctx, &name) {
        return ty;
    }

    if let Some(ty) = types::builtin_constant(&name) {
        return ty;
    }

    let mut candidates = glyph_scope_names(ctx);
    candidates.extend(top_level_names(ctx));
    let candidates_ref: Vec<&str> = candidates.iter().map(String::as_str).collect();
    ctx.error(resolve::unresolved_name(
        &name,
        span.into(),
        "this scope",
        candidates_ref.into_iter(),
    ))
}

/// Resolves `name` in the glyph-local value scope only (spec §5.11: the
/// glyph's own `let`s, path names, and anchor names), memoizing a `let`'s
/// type the same way [`top_level_type`] does.
pub(crate) fn glyph_local_type(ctx: &mut Ctx, key: &GlyphKey, name: &str) -> Option<Type> {
    let glyph = ctx.hir.glyphs.get(key)?;
    if glyph.anchors.contains_key(name) {
        return Some(Type::Pair);
    }
    if glyph.paths.iter().any(|p| p.name.as_deref() == Some(name)) {
        return Some(Type::Path);
    }
    if !glyph.lets.contains_key(name) {
        return None;
    }

    if ctx.glyph_visiting.contains(name) {
        return Some(Type::Error);
    }
    let let_decl = ctx.hir.glyphs.get(key)?.lets.get(name)?;
    if let Some(ty) = &let_decl.ty {
        return Some(ty.clone());
    }
    let value = let_decl.value.clone();

    ctx.glyph_visiting.insert(name.to_string());
    let ty = match value {
        Some(expr) => infer_expr(ctx, &expr),
        None => Type::Error,
    };
    ctx.glyph_visiting.shift_remove(name);

    if let Some(glyph) = ctx.hir.glyphs.get_mut(key)
        && let Some(let_decl) = glyph.lets.get_mut(name)
    {
        let_decl.ty = Some(ty.clone());
    }
    Some(ty)
}

/// Resolves `name` in the top-level value scope only (params, metrics,
/// top-level `let`s), memoizing a `let`'s type on first use.
pub(crate) fn top_level_type(ctx: &mut Ctx, name: &str) -> Option<Type> {
    if ctx.hir.params.contains_key(name) {
        return Some(Type::Num);
    }
    if ctx.hir.metrics.contains_key(name) {
        return Some(Type::Zone);
    }
    if !ctx.hir.lets.contains_key(name) {
        return None;
    }

    if ctx.top_visiting.contains(name) {
        return Some(Type::Error);
    }
    let let_decl = ctx.hir.lets.get(name)?;
    if let Some(ty) = &let_decl.ty {
        return Some(ty.clone());
    }
    let value = let_decl.value.clone();

    ctx.top_visiting.insert(name.to_string());
    let ty = match value {
        Some(expr) => infer_expr(ctx, &expr),
        None => Type::Error,
    };
    ctx.top_visiting.shift_remove(name);

    if let Some(let_decl) = ctx.hir.lets.get_mut(name) {
        let_decl.ty = Some(ty.clone());
    }
    Some(ty)
}

fn infer_tuple(ctx: &mut Ctx, tuple: &ast::TupleExpr) -> Type {
    let elements: Vec<Type> = tuple.elements().map(|e| infer_expr(ctx, &e)).collect();
    if elements.contains(&Type::Error) {
        return Type::Error;
    }
    if elements.len() == 2 && elements.iter().all(|t| *t == Type::Num) {
        return Type::Pair;
    }
    if elements.len() >= 2 && elements.iter().all(|t| *t == Type::Transform) {
        return Type::Transform;
    }
    let span = tuple.syntax().text_range();
    let described = elements
        .iter()
        .map(Type::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    ctx.error(Diagnostic::error(
        codes::MIXED_TUPLE,
        format!(
            "`({described})` is not a valid tuple; expected two `num`s or two or more `transform`s"
        ),
        mg_diag::Label::new(span.into(), "invalid tuple"),
    ))
}

fn infer_list(ctx: &mut Ctx, list: &ast::ListExpr) -> Type {
    let mut elem_ty: Option<Type> = None;
    for element in list.elements() {
        let ty = infer_expr(ctx, &element);
        if ty == Type::Error {
            return Type::Error;
        }
        match &elem_ty {
            None => elem_ty = Some(ty),
            Some(expected) if *expected == ty => {}
            Some(expected) => {
                let span = element.syntax().text_range();
                return ctx.error(Diagnostic::error(
                    codes::TYPE_MISMATCH,
                    format!(
                        "list elements must share one type: expected `{expected}`, found `{ty}`"
                    ),
                    mg_diag::Label::new(span.into(), "does not match earlier elements"),
                ));
            }
        }
    }
    Type::list_of(elem_ty.unwrap_or(Type::Error))
}

fn infer_unary(ctx: &mut Ctx, unary: &ast::UnaryExpr) -> Type {
    let Some(operand) = unary.operand() else {
        return Type::Error;
    };
    let operand_ty = infer_expr(ctx, &operand);
    let Some(op) = unary.op_token() else {
        return Type::Error;
    };
    if operand_ty == Type::Error {
        return Type::Error;
    }
    match (op.kind(), &operand_ty) {
        (SyntaxKind::MINUS, Type::Num | Type::Pair) => operand_ty,
        (SyntaxKind::NOT_KW, Type::Bool) => Type::Bool,
        _ => {
            let span = unary.syntax().text_range();
            ctx.error(Diagnostic::error(
                codes::TYPE_MISMATCH,
                format!("`{}` is not defined for `{operand_ty}`", op.text()),
                mg_diag::Label::new(span.into(), "invalid operand type"),
            ))
        }
    }
}

fn infer_bin(ctx: &mut Ctx, bin: &ast::BinExpr) -> Type {
    let (Some(lhs), Some(rhs), Some(op)) = (bin.lhs(), bin.rhs(), bin.op_token()) else {
        return Type::Error;
    };
    let lhs_ty = infer_expr(ctx, &lhs);
    let rhs_ty = infer_expr(ctx, &rhs);
    if lhs_ty == Type::Error || rhs_ty == Type::Error {
        return Type::Error;
    }

    use SyntaxKind::*;
    let arithmetic = matches!(op.kind(), PLUS | MINUS | STAR | SLASH | CARET);
    let comparison = matches!(op.kind(), LT | LE | GT | GE);
    let equality = matches!(op.kind(), EQEQ | NEQ);
    let logical = matches!(op.kind(), AND_KW | OR_KW);

    let result = match (&lhs_ty, &rhs_ty) {
        (Type::Num, Type::Num) if arithmetic => Some(Type::Num),
        (Type::Num, Type::Num) if comparison || equality => Some(Type::Bool),
        (Type::Pair, Type::Pair) if matches!(op.kind(), PLUS | MINUS) => Some(Type::Pair),
        (Type::Pair, Type::Pair) if equality => Some(Type::Bool),
        (Type::Pair, Type::Num) if matches!(op.kind(), STAR | SLASH) => Some(Type::Pair),
        (Type::Num, Type::Pair) if op.kind() == STAR => Some(Type::Pair),
        (Type::Bool, Type::Bool) if logical || equality => Some(Type::Bool),
        (Type::String, Type::String) if equality => Some(Type::Bool),
        _ => None,
    };

    match result {
        Some(ty) => ty,
        None => {
            let span = bin.syntax().text_range();
            ctx.error(Diagnostic::error(
                codes::TYPE_MISMATCH,
                format!(
                    "`{}` is not defined for `{lhs_ty}` and `{rhs_ty}`",
                    op.text()
                ),
                mg_diag::Label::new(span.into(), "invalid operand types"),
            ))
        }
    }
}

fn infer_call(ctx: &mut Ctx, call: &ast::CallExpr) -> Type {
    let args: Vec<ast::Expr> = call
        .arg_list()
        .map_or_else(Vec::new, |l| l.args().collect());
    let arg_types: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();

    let Some(ast::Expr::Ident(callee)) = call.callee() else {
        let span = call.syntax().text_range();
        return ctx.error(Diagnostic::error(
            codes::CALL_TO_NON_FUNCTION,
            "only a plain function name may be called",
            mg_diag::Label::new(span.into(), "not callable"),
        ));
    };
    let Some(name_token) = callee.token() else {
        return Type::Error;
    };
    let name = name_token.text().to_string();
    let span = name_token.text_range();

    let Some(sigs) = types::lookup_function(&name) else {
        return ctx.error(resolve::unresolved_name(
            &name,
            span.into(),
            "the function namespace",
            types::FUNCTION_NAMES.iter().copied(),
        ));
    };

    if arg_types.contains(&Type::Error) {
        return Type::Error;
    }

    if let Some(sig) = sigs
        .iter()
        .find(|sig| sig.params.len() == arg_types.len() && sig.params == arg_types)
    {
        return sig.ret.clone();
    }

    let call_span = call.syntax().text_range();
    if sigs.iter().all(|sig| sig.params.len() != arg_types.len()) {
        let arities: Vec<String> = sigs
            .iter()
            .map(|sig| sig.params.len().to_string())
            .collect();
        ctx.error(Diagnostic::error(
            codes::WRONG_ARGUMENT_COUNT,
            format!(
                "`{name}` takes {} argument(s), found {}",
                arities.join(" or "),
                arg_types.len()
            ),
            mg_diag::Label::new(call_span.into(), "wrong argument count"),
        ))
    } else {
        let expected = sigs
            .iter()
            .filter(|sig| sig.params.len() == arg_types.len())
            .map(|sig| {
                format!(
                    "({})",
                    sig.params
                        .iter()
                        .map(Type::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(" or ");
        let found = arg_types
            .iter()
            .map(Type::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        ctx.error(Diagnostic::error(
            codes::TYPE_MISMATCH,
            format!("`{name}` expected {expected}, found ({found})"),
            mg_diag::Label::new(call_span.into(), "argument type mismatch"),
        ))
    }
}

/// `receiver.member`. Handles the namespace-root special forms
/// (`font.*`, `instance.*`, `math.*`, `glyph.*`, `glyphs.<name>.*`, spec
/// §5.10) before falling back to the generic per-type member table.
fn infer_member(ctx: &mut Ctx, member: &ast::MemberExpr) -> Type {
    let (Some(receiver_expr), Some(member_token)) = (member.receiver(), member.member_token())
    else {
        return Type::Error;
    };
    let member_name = member_token.text().to_string();
    let member_span: std::ops::Range<usize> = member_token.text_range().into();

    if let ast::Expr::Ident(root) = &receiver_expr
        && let Some(root_token) = root.token()
    {
        match root_token.text() {
            "math" => {
                return match types::math_member(&member_name) {
                    Some(ty) => ty,
                    None => {
                        no_such_member(ctx, "math", &member_name, member_span, &["pi", "tau", "e"])
                    }
                };
            }
            "font" => {
                return match types::font_member(&member_name) {
                    Some(ty) => ty,
                    None => no_such_member(
                        ctx,
                        "font",
                        &member_name,
                        member_span,
                        &["name", "em", "version", "designer", "foundry", "license"],
                    ),
                };
            }
            "instance" => {
                return match types::instance_member(&member_name) {
                    Some(ty) => ty,
                    None => no_such_member(
                        ctx,
                        "instance",
                        &member_name,
                        member_span,
                        &["name", "slant"],
                    ),
                };
            }
            "glyph" => {
                if ctx.current_glyph.is_none() {
                    return ctx.error(Diagnostic::error(
                        codes::UNRESOLVED_NAME,
                        "`glyph` is only available inside a glyph's own body",
                        mg_diag::Label::new(root_token.text_range().into(), "not available here"),
                    ));
                }
                return match types::glyph_member(&member_name) {
                    Some(ty) => ty,
                    None => no_such_member(
                        ctx,
                        "glyph",
                        &member_name,
                        member_span,
                        &["name", "codepoints", "advance", "bbox"],
                    ),
                };
            }
            _ => {}
        }
    }

    if let ast::Expr::Member(inner) = &receiver_expr
        && let Some(ast::Expr::Ident(root)) = inner.receiver()
        && root.token().map(|t| t.text().to_string()) == Some("glyphs".to_string())
        && let Some(glyph_name_token) = inner.member_token()
    {
        return infer_glyphs_member(ctx, &glyph_name_token, &member_name, member_span);
    }

    let receiver_ty = infer_expr(ctx, &receiver_expr);
    if receiver_ty == Type::Error {
        return Type::Error;
    }
    match types::member_type(&receiver_ty, &member_name) {
        Some(ty) => ty,
        None => {
            let names = types::member_names(&receiver_ty);
            no_such_member(
                ctx,
                &receiver_ty.to_string(),
                &member_name,
                member_span,
                names,
            )
        }
    }
}

fn infer_glyphs_member(
    ctx: &mut Ctx,
    glyph_name_token: &mg_syntax::SyntaxToken,
    field: &str,
    field_span: std::ops::Range<usize>,
) -> Type {
    let glyph_name = glyph_name_token.text().to_string();
    let key: GlyphKey = (glyph_name.clone(), None);
    let Some(glyph) = ctx.hir.glyphs.get(&key) else {
        let candidates: Vec<String> = ctx
            .hir
            .glyphs
            .keys()
            .filter(|(_, set)| set.is_none())
            .map(|(name, _)| name.clone())
            .collect();
        let candidates_ref: Vec<&str> = candidates.iter().map(String::as_str).collect();
        return ctx.error(resolve::unresolved_name(
            &glyph_name,
            glyph_name_token.text_range().into(),
            "the glyph namespace",
            candidates_ref.into_iter(),
        ));
    };

    let is_anchor = glyph.anchors.contains_key(field);
    // Its named paths too (spec §5.10), in its placed coordinates.
    let is_path = glyph.path_named(field).is_some();
    let mut legal_names: Vec<String> = vec!["advance".to_string(), "bbox".to_string()];
    legal_names.extend(glyph.anchors.keys().cloned());
    legal_names.extend(glyph.paths.iter().filter_map(|p| p.name.clone()));

    match field {
        "advance" => Type::Num,
        "bbox" => Type::Rect,
        _ if is_anchor => Type::Pair,
        _ if is_path => Type::Path,
        _ => {
            let names_ref: Vec<&str> = legal_names.iter().map(String::as_str).collect();
            no_such_member(
                ctx,
                &format!("glyphs.{glyph_name}"),
                field,
                field_span,
                &names_ref,
            )
        }
    }
}

fn no_such_member(
    ctx: &mut Ctx,
    receiver_desc: &str,
    member: &str,
    span: std::ops::Range<usize>,
    legal: &[&str],
) -> Type {
    let diagnostic = Diagnostic::error(
        codes::NO_SUCH_MEMBER,
        format!("`{receiver_desc}` has no member `{member}`"),
        mg_diag::Label::new(span.clone(), "no such member"),
    );
    let diagnostic = match mg_diag::suggest::nearest_match(member, legal.iter().copied()) {
        Some(suggestion) => diagnostic.with_help(format!("did you mean `{suggestion}`?")),
        None => diagnostic,
    };
    ctx.error(diagnostic)
}
