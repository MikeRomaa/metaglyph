//! Static types of names and expressions, for member completion and
//! hover (plan 4, L2). Every type comes from the M2 type checker: a
//! `let`'s type is whatever `mg-hir` inferred for it, and member and
//! function result types come from `mg_hir::types`.
//!
//! The checker only runs on a file without syntax errors, and typing
//! `x.` always leaves one. So a document keeps the [`NameTypes`] of its
//! last error-free version and carries them into each broken edit.

use indexmap::IndexMap;
use mg_hir::Hir;
use mg_hir::types::{self, Type};
use mg_syntax::ast;

/// The type of every name the checker typed, by scope.
#[derive(Debug, Clone, Default)]
pub struct NameTypes {
    /// Params, metrics, and top-level `let`s.
    pub top: IndexMap<String, Type>,
    /// Per default-set glyph: its `let`s, anchors, and named paths.
    pub glyphs: IndexMap<String, IndexMap<String, Type>>,
}

impl NameTypes {
    pub fn from_hir(hir: &Hir) -> Self {
        let mut names = NameTypes::default();
        for name in hir.params.keys() {
            names.top.insert(name.clone(), Type::Num);
        }
        for name in hir.metrics.keys() {
            names.top.insert(name.clone(), Type::Zone);
        }
        for (name, decl) in &hir.lets {
            names
                .top
                .insert(name.clone(), decl.ty.clone().unwrap_or(Type::Error));
        }
        for ((glyph, set), decl) in &hir.glyphs {
            if set.is_some() {
                continue;
            }
            let scope = names.glyphs.entry(glyph.clone()).or_default();
            for (name, decl) in &decl.lets {
                scope.insert(name.clone(), decl.ty.clone().unwrap_or(Type::Error));
            }
            for name in decl.anchors.keys() {
                scope.insert(name.clone(), Type::Pair);
            }
            for path in &decl.paths {
                if let Some(name) = &path.name {
                    scope.insert(name.clone(), Type::Path);
                }
            }
        }
        names
    }

    /// A bare name's type, inside glyph `glyph` when there is one: glyph
    /// scope, then the top level, then the built-in constants (spec §5.11).
    pub fn name(&self, glyph: Option<&str>, name: &str) -> Option<Type> {
        glyph
            .and_then(|g| self.glyphs.get(g))
            .and_then(|scope| scope.get(name))
            .or_else(|| self.top.get(name))
            .cloned()
            .or_else(|| types::builtin_constant(name))
            .filter(|ty| *ty != Type::Error)
    }

    /// `expr`'s static type inside glyph `glyph`, as far as names and
    /// members determine it; `None` when it can't be known without
    /// evaluating.
    pub fn expr(&self, glyph: Option<&str>, expr: &ast::Expr) -> Option<Type> {
        match expr {
            ast::Expr::Ident(ident) => self.name(glyph, ident.token()?.text()),
            ast::Expr::Paren(paren) => self.expr(glyph, &paren.inner()?),
            ast::Expr::Tuple(tuple) if tuple.elements().count() == 2 => Some(Type::Pair),
            ast::Expr::Literal(lit) => {
                let kind = lit.token()?.kind();
                (kind != mg_syntax::SyntaxKind::STRING).then_some(Type::Num)
            }
            ast::Expr::Call(call) => {
                let ast::Expr::Ident(callee) = call.callee()? else {
                    return None;
                };
                let sigs = types::lookup_function(callee.token()?.text())?;
                Some(sigs.into_iter().next()?.ret)
            }
            ast::Expr::Member(member) => {
                let field = member.member_token()?;
                self.member(glyph, &member.receiver()?, field.text())
            }
            _ => None,
        }
    }

    /// The type of `receiver.member`.
    pub fn member(&self, glyph: Option<&str>, receiver: &ast::Expr, member: &str) -> Option<Type> {
        if let ast::Expr::Ident(root) = receiver {
            match root.token()?.text() {
                "font" => return types::font_member(member),
                "glyph" => return types::glyph_member(member),
                "instance" => return types::instance_member(member),
                "math" => return types::math_member(member),
                _ => {}
            }
        }
        if let Some(target) = glyphs_member_target(receiver) {
            return match member {
                "advance" => Some(Type::Num),
                "bbox" => Some(Type::Rect),
                anchor => self
                    .glyphs
                    .get(&target)
                    .and_then(|scope| scope.get(anchor))
                    .filter(|ty| **ty == Type::Pair)
                    .cloned(),
            };
        }
        types::member_type(&self.expr(glyph, receiver)?, member)
    }
}

/// `X`, when `expr` is `glyphs.X`.
pub fn glyphs_member_target(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::Member(inner) = expr else {
        return None;
    };
    let ast::Expr::Ident(root) = inner.receiver()? else {
        return None;
    };
    (root.token()?.text() == "glyphs")
        .then(|| inner.member_token().map(|t| t.text().to_string()))?
}
