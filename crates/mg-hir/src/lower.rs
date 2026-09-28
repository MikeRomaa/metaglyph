//! CST → HIR lowering (spec §5.3–§5.8, §5.11), in two passes.
//!
//! Pass 1 (`lower_structure`) walks the CST once, building every
//! declaration's shape: field presence/mutex/required via `crate::schema`,
//! names registered into their namespace via `crate::resolve`, path bodies
//! validated structurally via `crate::path_check`, and every field's value
//! kept as an `ast::Expr` handle. None of this needs a name to resolve
//! anywhere outside the current declaration, so it can run in source
//! order without knowing what comes later.
//!
//! Pass 2 (`typecheck_and_resolve`) needs the opposite: every name and
//! every glyph's anchor set must already exist, because declaration order
//! is irrelevant (spec §5.11 rule 7) and a `let` may reference one that is
//! declared textually later, or a glyph body may reference
//! `glyphs.<name>` for a glyph declared anywhere else in the file. It
//! type-checks every field, resolves every `glyphref`/`groupref`/`pathref`,
//! and runs the checks that need the whole font in view at once
//! (duplicate/case-fold glyph names, kerning group overlap, required
//! metrics, range membership).

use std::ops::Range;

use indexmap::{IndexMap, IndexSet};
use mg_diag::{Diagnostic, Label};
use mg_syntax::SyntaxNode;
use mg_syntax::ast::{self, AstNode};
use mg_syntax::syntax_kind::SyntaxKind;

use crate::const_eval;
use crate::model::*;
use crate::resolve::{self, ValueNamespace};
use crate::schema;
use crate::type_check::{self, Ctx};
use crate::types::Type;
use mg_diag::codes;

pub(crate) fn lower(source_file: &ast::SourceFile) -> (Hir, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut hir = lower_structure(source_file, &mut diagnostics);
    typecheck_and_resolve(&mut hir, &mut diagnostics);
    (hir, diagnostics)
}

// ---------------------------------------------------------------------
// Small shared helpers

/// The plain identifier a field's value must be, for `identifier`,
/// `glyphref`, `groupref`, and `pathref`-typed fields (spec §5.2: these
/// have "syntax given with the field," not the general expression grammar
/// — but the parser cannot tell them apart from an ordinary bare-name
/// expression at parse time, so this is where the distinction is made).
fn ident_text(expr: &ast::Expr) -> Option<(String, Range<usize>)> {
    match expr {
        ast::Expr::Ident(ident) => {
            let token = ident.token()?;
            Some((token.text().to_string(), token.text_range().into()))
        }
        _ => None,
    }
}

fn expect_ident_field(
    fields: &IndexMap<String, ast::Field>,
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, Range<usize>)> {
    let field = fields.get(name)?;
    let expr = field.value()?;
    match ident_text(&expr) {
        Some(result) => Some(result),
        None => {
            diagnostics.push(Diagnostic::error(
                codes::TYPE_MISMATCH,
                format!("`{name}` must be a plain name"),
                Label::new(expr.syntax().text_range().into(), "expected a name"),
            ));
            None
        }
    }
}

/// The unescaped value of a `"…"` literal (spec §5.1: `\"` `\\` `\n` `\t`).
fn string_literal_value(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::Literal(lit) = expr else {
        return None;
    };
    lit.string_value()
}

/// A field whose value must be one of `legal`, defaulting to
/// `default` when the field is absent (spec §5.5: "Validation is per
/// field, so a diagnostic enumerates the legal set").
fn enum_field(
    fields: &IndexMap<String, ast::Field>,
    table: &'static [schema::FieldSchema],
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let entry = schema::entry(table, name);
    let legal = entry.values;
    let default = entry
        .default
        .expect("an enum field with a default")
        .trim_matches('"');
    let Some(field) = fields.get(name) else {
        return default.to_string();
    };
    let Some(expr) = field.value() else {
        return default.to_string();
    };
    match string_literal_value(&expr) {
        Some(value) if legal.contains(&value.as_str()) => value,
        Some(value) => {
            diagnostics.push(Diagnostic::error(
                codes::UNKNOWN_ENUM_VALUE,
                format!(
                    "unknown {name} \"{value}\"; expected one of: {}",
                    legal.join(", ")
                ),
                Label::new(expr.syntax().text_range().into(), "not a legal value"),
            ));
            default.to_string()
        }
        None => {
            diagnostics.push(Diagnostic::error(
                codes::TYPE_MISMATCH,
                format!("`{name}` must be a string literal"),
                Label::new(expr.syntax().text_range().into(), "expected a string"),
            ));
            default.to_string()
        }
    }
}

fn missing_name(node: &SyntaxNode, kind_desc: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(Diagnostic::error(
        codes::MISSING_DECLARATION_NAME,
        format!("{kind_desc} requires a name"),
        Label::new(node.text_range().into(), "missing name"),
    ));
}

// ---------------------------------------------------------------------
// Pass 1: structure

fn lower_structure(source_file: &ast::SourceFile, diagnostics: &mut Vec<Diagnostic>) -> Hir {
    let items: Vec<SyntaxNode> = source_file.items().collect();

    let font = lower_font(source_file, &items, diagnostics);
    let font_em = font.em.map(|v| v as f64);

    // Param names must be known before any `instance` is processed, since
    // declaration order is irrelevant (spec §5.11 rule 7) and an instance
    // may textually precede the params it overrides.
    let declared_param_names: IndexSet<String> = items
        .iter()
        .filter(|n| n.kind() == SyntaxKind::PARAM)
        .filter_map(|n| ast::Param::cast(n.clone()))
        .filter_map(|p| p.name_token())
        .map(|t| t.text().to_string())
        .collect();

    let mut top_level = ValueNamespace::new();
    let mut params = IndexMap::new();
    let mut metrics = IndexMap::new();
    let mut lets = IndexMap::new();
    let mut glyphs: IndexMap<GlyphKey, GlyphDecl> = IndexMap::new();
    let mut instances = IndexMap::new();
    let mut groups = IndexMap::new();
    let mut kerns = Vec::new();

    for item in &items {
        match item.kind() {
            SyntaxKind::PARAM => {
                if let Some(decl) = lower_param(item.clone(), &mut top_level, font_em, diagnostics)
                {
                    params.insert(decl.name.clone(), decl);
                }
            }
            SyntaxKind::METRIC => {
                if let Some(decl) = lower_metric(item.clone(), &mut top_level, diagnostics) {
                    metrics.insert(decl.name.clone(), decl);
                }
            }
            SyntaxKind::LET_STMT => {
                let let_stmt = ast::LetStmt::cast(item.clone()).expect("LET_STMT casts");
                if let Some(token) = let_stmt.name_token() {
                    let name = token.text().to_string();
                    if top_level.declare(&name, token.text_range().into(), None, diagnostics) {
                        lets.insert(
                            name.clone(),
                            LetDecl {
                                name,
                                syntax: let_stmt.syntax().clone(),
                                value: let_stmt.value(),
                                ty: None,
                            },
                        );
                    }
                }
            }
            SyntaxKind::GLYPH => {
                let glyph_node = ast::Glyph::cast(item.clone()).expect("GLYPH casts");
                if let Some((key, decl)) = lower_glyph(glyph_node, &top_level, font_em, diagnostics)
                {
                    if glyphs.contains_key(&key) {
                        let (name, set) = &key;
                        let desc = match set {
                            Some(set) => format!("`{name}` in glyph set `{set}`"),
                            None => format!("`{name}`"),
                        };
                        diagnostics.push(Diagnostic::error(
                            codes::DUPLICATE_DEFINITION,
                            format!("glyph {desc} is already defined"),
                            Label::new(decl.syntax.text_range().into(), "duplicate definition"),
                        ));
                    } else {
                        glyphs.insert(key, decl);
                    }
                }
            }
            SyntaxKind::INSTANCE => {
                let instance_node = ast::Instance::cast(item.clone()).expect("INSTANCE casts");
                if let Some(decl) =
                    lower_instance(instance_node, &declared_param_names, diagnostics)
                {
                    if instances.contains_key(&decl.name) {
                        diagnostics.push(Diagnostic::error(
                            codes::DUPLICATE_DEFINITION,
                            format!("instance `{}` is already defined", decl.name),
                            Label::new(decl.syntax.text_range().into(), "duplicate definition"),
                        ));
                    } else {
                        instances.insert(decl.name.clone(), decl);
                    }
                }
            }
            SyntaxKind::GROUP => {
                let group_node = ast::Group::cast(item.clone()).expect("GROUP casts");
                if let Some(decl) = lower_group(group_node, diagnostics) {
                    if groups.contains_key(&decl.name) {
                        diagnostics.push(Diagnostic::error(
                            codes::DUPLICATE_DEFINITION,
                            format!("group `{}` is already defined", decl.name),
                            Label::new(decl.syntax.text_range().into(), "duplicate definition"),
                        ));
                    } else {
                        groups.insert(decl.name.clone(), decl);
                    }
                }
            }
            SyntaxKind::KERN => {
                let kern_node = ast::Kern::cast(item.clone()).expect("KERN casts");
                kerns.push(lower_kern(kern_node, diagnostics));
            }
            _ => {}
        }
    }

    if instances.is_empty() {
        instances.insert(
            "Regular".to_string(),
            InstanceDecl {
                name: "Regular".to_string(),
                syntax: source_file.syntax().clone(),
                overrides: IndexMap::new(),
                slant: None,
                glyphset: None,
                style_name: "Regular".to_string(),
                weight_class: 400,
                width_class: 5,
            },
        );
    }

    Hir {
        font,
        params,
        metrics,
        lets,
        glyphs,
        instances,
        groups,
        kerns,
    }
}

fn lower_font(
    source_file: &ast::SourceFile,
    items: &[SyntaxNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> FontDecl {
    let font_nodes: Vec<ast::Font> = items
        .iter()
        .filter(|n| n.kind() == SyntaxKind::FONT)
        .filter_map(|n| ast::Font::cast(n.clone()))
        .collect();

    for extra in font_nodes.iter().skip(1) {
        diagnostics.push(Diagnostic::error(
            codes::DUPLICATE_DEFINITION,
            "a font may declare only one `font (...)`",
            Label::new(extra.syntax().text_range().into(), "duplicate `font`"),
        ));
    }

    let Some(font_node) = font_nodes.into_iter().next() else {
        let span: Range<usize> = match items.first() {
            Some(first) => first.text_range().into(),
            None => 0..0,
        };
        diagnostics.push(Diagnostic::error(
            codes::MISSING_REQUIRED_FIELD,
            "a font must declare exactly one `font (...)`",
            Label::new(span, "no `font` declaration"),
        ));
        return FontDecl {
            syntax: source_file.syntax().clone(),
            name: None,
            em: None,
            version: "1.000".to_string(),
            designer: None,
            foundry: None,
            license: None,
        };
    };

    let fields = schema::collect_fields(
        font_node.syntax(),
        font_node.config().as_ref(),
        schema::FONT_FIELDS,
        "`font`",
        diagnostics,
    );

    let name = fields
        .get("name")
        .and_then(|f| f.value())
        .and_then(|e| string_literal_value(&e));

    let em = fields.get("em").and_then(|f| f.value()).and_then(|expr| {
        let value = const_eval::eval_const(&expr, None)?;
        if value.fract() != 0.0 {
            diagnostics.push(Diagnostic::error(
                codes::NON_INTEGRAL_INT_FIELD,
                format!("`em` must be an exact integer, found {value}"),
                Label::new(expr.syntax().text_range().into(), "not an integer"),
            ));
            return None;
        }
        let int_value = value as i64;
        if !(16..=16384).contains(&int_value) {
            diagnostics.push(Diagnostic::error(
                codes::VALUE_OUT_OF_RANGE,
                format!("`em` must be between 16 and 16384, found {int_value}"),
                Label::new(expr.syntax().text_range().into(), "out of range"),
            ));
            return None;
        }
        Some(int_value)
    });

    let version = fields
        .get("version")
        .and_then(|f| f.value())
        .and_then(|e| {
            let value = string_literal_value(&e)?;
            // spec §5.6: form `digits.digits`, since it feeds
            // `head.fontRevision` as a number.
            let well_formed = value.split_once('.').is_some_and(|(whole, frac)| {
                !whole.is_empty()
                    && !frac.is_empty()
                    && whole.bytes().all(|b| b.is_ascii_digit())
                    && frac.bytes().all(|b| b.is_ascii_digit())
            });
            if !well_formed {
                diagnostics.push(
                    Diagnostic::error(
                        codes::MALFORMED_VERSION,
                        format!("`version` must have the form `digits.digits`, found {value:?}"),
                        Label::new(e.syntax().text_range().into(), "not `digits.digits`"),
                    )
                    .with_help("write just the number, e.g. `version: \"1.000\"`; `mg build` adds \"Version \" in the name table itself"),
                );
                return None;
            }
            Some(value)
        })
        .unwrap_or_else(|| "1.000".to_string());

    let designer = fields
        .get("designer")
        .and_then(|f| f.value())
        .and_then(|e| string_literal_value(&e));
    let foundry = fields
        .get("foundry")
        .and_then(|f| f.value())
        .and_then(|e| string_literal_value(&e));
    let license = fields
        .get("license")
        .and_then(|f| f.value())
        .and_then(|e| string_literal_value(&e));

    FontDecl {
        syntax: font_node.syntax().clone(),
        name,
        em,
        version,
        designer,
        foundry,
        license,
    }
}

fn lower_param(
    node: SyntaxNode,
    top_level: &mut ValueNamespace,
    font_em: Option<f64>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ParamDecl> {
    let param_node = ast::Param::cast(node).expect("PARAM casts");
    let Some(name_token) = param_node.name_token() else {
        missing_name(param_node.syntax(), "`param`", diagnostics);
        return None;
    };
    let name = name_token.text().to_string();

    const INSTANCE_FIELD_NAMES: &[&str] = &[
        "slant",
        "glyphset",
        "styleName",
        "weightClass",
        "widthClass",
    ];
    if INSTANCE_FIELD_NAMES.contains(&name.as_str()) {
        diagnostics.push(Diagnostic::error(
            codes::PARAM_NAMED_AFTER_INSTANCE_FIELD,
            format!("`{name}` may not be used as a param name; it is an instance field"),
            Label::new(name_token.text_range().into(), "reserved for `instance`"),
        ));
    }

    if !top_level.declare(&name, name_token.text_range().into(), None, diagnostics) {
        return None;
    }

    let fields = schema::collect_fields(
        param_node.syntax(),
        param_node.config().as_ref(),
        schema::PARAM_FIELDS,
        "`param`",
        diagnostics,
    );

    let default_expr = fields.get("default").and_then(|f| f.value());
    let default_value =
        default_expr
            .as_ref()
            .and_then(|expr| match const_eval::eval_const(expr, font_em) {
                Some(v) => Some(v),
                None => {
                    diagnostics.push(Diagnostic::error(
                        codes::NON_CONSTANT_EXPRESSION,
                        "`default` must be a constant expression",
                        Label::new(expr.syntax().text_range().into(), "not constant"),
                    ));
                    None
                }
            });

    let range = fields
        .get("range")
        .and_then(|f| f.value())
        .and_then(|expr| {
            let ast::Expr::Range(range_expr) = expr else {
                diagnostics.push(Diagnostic::error(
                    codes::TYPE_MISMATCH,
                    "`range` must be `low..high`",
                    Label::new(expr.syntax().text_range().into(), "expected a range"),
                ));
                return None;
            };
            let low = range_expr
                .low()
                .and_then(|e| const_eval::eval_const(&e, font_em));
            let high = range_expr
                .high()
                .and_then(|e| const_eval::eval_const(&e, font_em));
            match (low, high) {
                (Some(low), Some(high)) => Some((low, high)),
                _ => {
                    diagnostics.push(Diagnostic::error(
                        codes::NON_CONSTANT_EXPRESSION,
                        "a range's bounds must be constant",
                        Label::new(range_expr.syntax().text_range().into(), "not constant"),
                    ));
                    None
                }
            }
        });

    if let (Some(value), Some((low, high))) = (default_value, range)
        && (value < low || value > high)
    {
        diagnostics.push(Diagnostic::error(
            codes::VALUE_OUT_OF_RANGE,
            format!("`default` ({value}) lies outside `range` ({low}..{high})"),
            Label::new(
                default_expr.as_ref().unwrap().syntax().text_range().into(),
                "outside range",
            ),
        ));
    }

    Some(ParamDecl {
        name,
        syntax: param_node.syntax().clone(),
        default: default_expr,
        default_value,
        range,
    })
}

fn lower_metric(
    node: SyntaxNode,
    top_level: &mut ValueNamespace,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<MetricDecl> {
    let metric_node = ast::Metric::cast(node).expect("METRIC casts");
    let Some(name_token) = metric_node.name_token() else {
        missing_name(metric_node.syntax(), "`metric`", diagnostics);
        return None;
    };
    let name = name_token.text().to_string();

    if !top_level.declare(&name, name_token.text_range().into(), None, diagnostics) {
        return None;
    }

    let fields = schema::collect_fields(
        metric_node.syntax(),
        metric_node.config().as_ref(),
        schema::METRIC_FIELDS,
        "`metric`",
        diagnostics,
    );

    let y = fields.get("y").and_then(|f| f.value());
    let overshoot = fields.get("overshoot").and_then(|f| f.value());
    let align_text = enum_field(&fields, schema::METRIC_FIELDS, "align", diagnostics);
    let align = if align_text == "bottom" {
        Align::Bottom
    } else {
        Align::Top
    };

    Some(MetricDecl {
        name,
        syntax: metric_node.syntax().clone(),
        y,
        overshoot,
        align,
    })
}

fn lower_glyph(
    glyph_node: ast::Glyph,
    top_level: &ValueNamespace,
    font_em: Option<f64>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(GlyphKey, GlyphDecl)> {
    let Some(name_token) = glyph_node.name_token() else {
        missing_name(glyph_node.syntax(), "`glyph`", diagnostics);
        return None;
    };
    let name = name_token.text().to_string();

    let fields = schema::collect_fields(
        glyph_node.syntax(),
        glyph_node.config().as_ref(),
        schema::GLYPH_FIELDS,
        "`glyph`",
        diagnostics,
    );

    let glyphset = expect_ident_field(&fields, "glyphset", diagnostics).map(|(name, _)| name);
    let advance = fields.get("advance").and_then(|f| f.value());
    let codepoint_expr = fields.get("codepoint").and_then(|f| f.value());
    let codepoints = codepoint_expr
        .as_ref()
        .map(|expr| lower_codepoints(expr, font_em, diagnostics))
        .unwrap_or_default();

    let mut glyph_scope = ValueNamespace::new();
    let mut lets = IndexMap::new();
    let mut raw_paths: Vec<(ast::Path, IndexMap<String, ast::Field>)> = Vec::new();
    let mut anchors = IndexMap::new();
    let mut components = Vec::new();

    if let Some(body) = glyph_node.body() {
        for item in body.items() {
            match item.kind() {
                SyntaxKind::LET_STMT => {
                    let let_stmt = ast::LetStmt::cast(item).expect("LET_STMT casts");
                    if let Some(token) = let_stmt.name_token() {
                        let nm = token.text().to_string();
                        if glyph_scope.declare(
                            &nm,
                            token.text_range().into(),
                            Some(top_level),
                            diagnostics,
                        ) {
                            lets.insert(
                                nm.clone(),
                                LetDecl {
                                    name: nm,
                                    syntax: let_stmt.syntax().clone(),
                                    value: let_stmt.value(),
                                    ty: None,
                                },
                            );
                        }
                    }
                }
                SyntaxKind::PATH => {
                    let path_node = ast::Path::cast(item).expect("PATH casts");
                    if let Some(token) = path_node.name_token() {
                        glyph_scope.declare(
                            token.text(),
                            token.text_range().into(),
                            Some(top_level),
                            diagnostics,
                        );
                    }
                    let path_fields = schema::collect_fields(
                        path_node.syntax(),
                        path_node.config().as_ref(),
                        schema::PATH_FIELDS,
                        "`path`",
                        diagnostics,
                    );
                    raw_paths.push((path_node, path_fields));
                }
                SyntaxKind::ANCHOR => {
                    let anchor_node = ast::Anchor::cast(item).expect("ANCHOR casts");
                    match anchor_node.name_token() {
                        Some(token) => {
                            let nm = token.text().to_string();
                            let span: Range<usize> = token.text_range().into();
                            if matches!(nm.as_str(), "advance" | "bbox") {
                                diagnostics.push(Diagnostic::error(
                                    codes::RESERVED_ANCHOR_NAME,
                                    format!("an anchor may not be named `{nm}`"),
                                    Label::new(span, "reserved anchor name"),
                                ));
                            } else if glyph_scope.declare(&nm, span, Some(top_level), diagnostics) {
                                let anchor_fields = schema::collect_fields(
                                    anchor_node.syntax(),
                                    anchor_node.config().as_ref(),
                                    schema::ANCHOR_FIELDS,
                                    "`anchor`",
                                    diagnostics,
                                );
                                let at = anchor_fields.get("at").and_then(|f| f.value());
                                anchors.insert(
                                    nm.clone(),
                                    AnchorDecl {
                                        name: nm,
                                        syntax: anchor_node.syntax().clone(),
                                        at,
                                    },
                                );
                            }
                        }
                        None => missing_name(anchor_node.syntax(), "`anchor`", diagnostics),
                    }
                }
                SyntaxKind::COMPONENT => {
                    let comp_node = ast::Component::cast(item).expect("COMPONENT casts");
                    let comp_fields = schema::collect_fields(
                        comp_node.syntax(),
                        comp_node.config().as_ref(),
                        schema::COMPONENT_FIELDS,
                        "`component`",
                        diagnostics,
                    );
                    let glyph_ref =
                        expect_ident_field(&comp_fields, "glyph", diagnostics).map(|(n, _)| n);
                    let offset = comp_fields.get("offset").and_then(|f| f.value());
                    let transform = comp_fields.get("transform").and_then(|f| f.value());
                    components.push(ComponentDecl {
                        syntax: comp_node.syntax().clone(),
                        glyph: glyph_ref,
                        offset,
                        transform,
                    });
                }
                _ => {}
            }
        }
    }

    let paths: Vec<PathDecl> = raw_paths
        .into_iter()
        .map(|(node, fields)| lower_path(node, fields, diagnostics))
        .collect();
    for path in &paths {
        crate::path_check::check_path(&paths, path, diagnostics);
    }

    let key = (name.clone(), glyphset.clone());
    Some((
        key,
        GlyphDecl {
            name,
            glyphset,
            syntax: glyph_node.syntax().clone(),
            codepoint_expr,
            codepoints,
            advance,
            lets,
            paths,
            anchors,
            components,
        },
    ))
}

/// `codepoint` (spec §5.6): an `int` or `int*` field, each value a
/// constant expression in `0`..=`0x10FFFF`.
fn lower_codepoints(
    expr: &ast::Expr,
    font_em: Option<f64>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<u32> {
    let elements: Vec<ast::Expr> = match expr {
        ast::Expr::List(list) => list.elements().collect(),
        other => vec![other.clone()],
    };

    let mut codepoints = Vec::new();
    for element in &elements {
        let span: Range<usize> = element.syntax().text_range().into();
        let Some(value) = const_eval::eval_const(element, font_em) else {
            diagnostics.push(Diagnostic::error(
                codes::NON_CONSTANT_EXPRESSION,
                "`codepoint` must be a constant expression",
                Label::new(span, "not constant"),
            ));
            continue;
        };
        if value.fract() != 0.0 {
            diagnostics.push(Diagnostic::error(
                codes::NON_INTEGRAL_INT_FIELD,
                format!("`codepoint` must be an exact integer, found {value}"),
                Label::new(span, "not an integer"),
            ));
            continue;
        }
        if !(0.0..=0x10FFFF as f64).contains(&value) {
            diagnostics.push(Diagnostic::error(
                codes::CODEPOINT_OUT_OF_RANGE,
                format!("codepoint {value} is outside 0..=U+10FFFF"),
                Label::new(span, "out of range"),
            ));
            continue;
        }
        codepoints.push(value as u32);
    }
    codepoints
}

fn lower_path(
    path_node: ast::Path,
    fields: IndexMap<String, ast::Field>,
    diagnostics: &mut Vec<Diagnostic>,
) -> PathDecl {
    let name = path_node.name_token().map(|t| t.text().to_string());

    let follows = expect_ident_field(&fields, "follows", diagnostics).map(|(n, _)| n);
    let stroke = fields.get("stroke").and_then(|f| f.value());
    let fill = fields
        .get("fill")
        .and_then(|f| f.value())
        .and_then(|e| const_eval::eval_const_bool(&e))
        .unwrap_or(false);
    let enabled = fields
        .get("enabled")
        .and_then(|f| f.value())
        .and_then(|e| const_eval::eval_const_bool(&e))
        .unwrap_or(true);
    let joins = enum_field(&fields, schema::PATH_FIELDS, "joins", diagnostics);

    let caps = fields
        .get("caps")
        .and_then(|f| f.value())
        .map(|expr| lower_caps(&expr, diagnostics));

    let raw_join_at = fields.get("joinAt").and_then(|f| f.value());

    let (segments, closed) = match path_node.body() {
        Some(body) => lower_path_body(body, diagnostics),
        None => (Vec::new(), false),
    };

    let join_at = raw_join_at
        .map(|expr| lower_join_at(&expr, &segments, diagnostics))
        .unwrap_or_default();

    PathDecl {
        name,
        syntax: path_node.syntax().clone(),
        follows,
        stroke,
        fill,
        caps,
        joins,
        join_at,
        enabled,
        segments,
        closed,
    }
}

/// `caps` (spec §5.7): a bare string sets both ends, a 2-tuple sets
/// `(start, end)` — the spec's "string pair" type (§5.5), legal only
/// here, so (like `sweep`/`joins`/`align`) this matches the AST shape by
/// hand rather than going through the general expression type-checker.
fn lower_caps(expr: &ast::Expr, diagnostics: &mut Vec<Diagnostic>) -> CapsSpec {
    let default = schema::entry(schema::PATH_FIELDS, "caps")
        .default
        .expect("`caps` has a default")
        .trim_matches('"');

    match expr {
        ast::Expr::Tuple(tuple) => {
            let elements: Vec<ast::Expr> = tuple.elements().collect();
            if elements.len() != 2 {
                diagnostics.push(Diagnostic::error(
                    codes::TYPE_MISMATCH,
                    "`caps` tuple must have exactly two elements: `(start, end)`",
                    Label::new(expr.syntax().text_range().into(), "expected a 2-tuple"),
                ));
                return CapsSpec {
                    start: default.to_string(),
                    end: default.to_string(),
                };
            }
            let start = lower_cap_value(&elements[0], diagnostics).unwrap_or(default.to_string());
            let end = lower_cap_value(&elements[1], diagnostics).unwrap_or(default.to_string());
            CapsSpec { start, end }
        }
        _ => {
            let value = lower_cap_value(expr, diagnostics).unwrap_or(default.to_string());
            CapsSpec {
                start: value.clone(),
                end: value,
            }
        }
    }
}

fn lower_cap_value(expr: &ast::Expr, diagnostics: &mut Vec<Diagnostic>) -> Option<String> {
    let legal = schema::CAP_VALUES;
    let Some(value) = string_literal_value(expr) else {
        diagnostics.push(Diagnostic::error(
            codes::TYPE_MISMATCH,
            "`caps` must be a string literal or a 2-tuple of string literals",
            Label::new(expr.syntax().text_range().into(), "expected a string"),
        ));
        return None;
    };
    if !legal.contains(&value.as_str()) {
        diagnostics.push(Diagnostic::error(
            codes::UNKNOWN_ENUM_VALUE,
            format!(
                "unknown cap \"{value}\"; expected one of: {}",
                legal.join(", ")
            ),
            Label::new(expr.syntax().text_range().into(), "not a legal value"),
        ));
        return None;
    }
    Some(value)
}

fn lower_join_at(
    expr: &ast::Expr,
    segments: &[SegmentDecl],
    diagnostics: &mut Vec<Diagnostic>,
) -> IndexMap<String, String> {
    let mut result = IndexMap::new();
    let legal = schema::JOIN_VALUES;

    let ast::Expr::Map(map) = expr else {
        diagnostics.push(Diagnostic::error(
            codes::TYPE_MISMATCH,
            "`joinAt` must be a map from segment name to join kind",
            Label::new(expr.syntax().text_range().into(), "expected a map"),
        ));
        return result;
    };

    let segment_names: Vec<&str> = segments.iter().filter_map(|s| s.name.as_deref()).collect();

    for entry in map.entries() {
        let Some(key_token) = entry.key_token() else {
            continue;
        };
        let key = key_token.text().to_string();
        let Some(value_expr) = entry.value() else {
            continue;
        };
        if !segment_names.contains(&key.as_str()) {
            let diagnostic = Diagnostic::error(
                codes::UNRESOLVED_NAME,
                format!("`{key}` is not a segment of this path"),
                Label::new(key_token.text_range().into(), "not found"),
            );
            let diagnostic =
                match mg_diag::suggest::nearest_match(&key, segment_names.iter().copied()) {
                    Some(s) => diagnostic.with_help(format!("did you mean `{s}`?")),
                    None => diagnostic,
                };
            diagnostics.push(diagnostic);
            continue;
        }
        let Some(value) = string_literal_value(&value_expr) else {
            diagnostics.push(Diagnostic::error(
                codes::TYPE_MISMATCH,
                "`joinAt` values must be string literals",
                Label::new(value_expr.syntax().text_range().into(), "expected a string"),
            ));
            continue;
        };
        if !legal.contains(&value.as_str()) {
            diagnostics.push(Diagnostic::error(
                codes::UNKNOWN_ENUM_VALUE,
                format!(
                    "unknown join \"{value}\"; expected one of: {}",
                    legal.join(", ")
                ),
                Label::new(value_expr.syntax().text_range().into(), "not a legal value"),
            ));
            continue;
        }
        result.insert(key, value);
    }

    result
}

fn lower_path_body(body: ast::Body, diagnostics: &mut Vec<Diagnostic>) -> (Vec<SegmentDecl>, bool) {
    let mut segments = Vec::new();
    let mut closed = false;

    for item in body.items() {
        match item.kind() {
            SyntaxKind::START
            | SyntaxKind::LINE
            | SyntaxKind::QUAD
            | SyntaxKind::CUBE
            | SyntaxKind::ARC => {
                segments.push(lower_segment(item, diagnostics));
            }
            SyntaxKind::CLOSE => {
                closed = true;
                let close_node = ast::Close::cast(item).expect("CLOSE casts");
                schema::collect_fields(
                    close_node.syntax(),
                    close_node.config().as_ref(),
                    schema::CLOSE_FIELDS,
                    "`close`",
                    diagnostics,
                );
            }
            _ => {}
        }
    }

    (segments, closed)
}

/// `sweep` (spec §5.7): a required `"ccw"`/`"cw"` enum, resolved at HIR
/// time like `joins`/`align` rather than deferred to evaluation.
fn lower_sweep(
    fields: &IndexMap<String, ast::Field>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Sweep> {
    let field = fields.get("sweep")?;
    let expr = field.value()?;
    let legal = schema::entry(schema::ARC_FIELDS, "sweep").values;
    match string_literal_value(&expr) {
        Some(value) if value == Sweep::Ccw.as_str() => Some(Sweep::Ccw),
        Some(value) if value == Sweep::Cw.as_str() => Some(Sweep::Cw),
        Some(value) => {
            diagnostics.push(Diagnostic::error(
                codes::UNKNOWN_ENUM_VALUE,
                format!(
                    "unknown sweep \"{value}\"; expected one of: {}",
                    legal.join(", ")
                ),
                Label::new(expr.syntax().text_range().into(), "not a legal value"),
            ));
            None
        }
        None => {
            diagnostics.push(Diagnostic::error(
                codes::TYPE_MISMATCH,
                "`sweep` must be a string literal",
                Label::new(expr.syntax().text_range().into(), "expected a string"),
            ));
            None
        }
    }
}

/// `arc`'s two modes (spec §6.3): `crate::schema::ARC_FIELDS` already
/// rejects mixing `center` with `rx`/`ry` and requires `rx` and `ry`
/// together, but "at least one mode" is a cross-group rule the field
/// table can't express, so it's checked here instead.
fn check_arc_mode(
    fields: &IndexMap<String, ast::Field>,
    block_syntax: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !fields.contains_key("center") && !fields.contains_key("rx") {
        diagnostics.push(Diagnostic::error(
            codes::MISSING_REQUIRED_FIELD,
            "`arc` needs either `center`, or `rx` and `ry`",
            Label::new(schema::trimmed_span(block_syntax), "missing arc geometry"),
        ));
    }
}

fn lower_segment(node: SyntaxNode, diagnostics: &mut Vec<Diagnostic>) -> SegmentDecl {
    match node.kind() {
        SyntaxKind::START => {
            let start = ast::Start::cast(node).expect("START casts");
            let fields = schema::collect_fields(
                start.syntax(),
                start.config().as_ref(),
                schema::START_FIELDS,
                "`start`",
                diagnostics,
            );
            SegmentDecl {
                kind: SegmentKind::Start,
                name: start.name_token().map(|t| t.text().to_string()),
                syntax: start.syntax().clone(),
                at: fields.get("at").and_then(|f| f.value()),
                to: None,
                c: None,
                c1: None,
                c2: None,
                center: None,
                rx: None,
                ry: None,
                large: false,
                sweep: None,
            }
        }
        SyntaxKind::LINE => {
            let line = ast::Line::cast(node).expect("LINE casts");
            let fields = schema::collect_fields(
                line.syntax(),
                line.config().as_ref(),
                schema::LINE_FIELDS,
                "`line`",
                diagnostics,
            );
            SegmentDecl {
                kind: SegmentKind::Line,
                name: line.name_token().map(|t| t.text().to_string()),
                syntax: line.syntax().clone(),
                at: None,
                to: fields.get("to").and_then(|f| f.value()),
                c: None,
                c1: None,
                c2: None,
                center: None,
                rx: None,
                ry: None,
                large: false,
                sweep: None,
            }
        }
        SyntaxKind::QUAD => {
            let quad = ast::Quad::cast(node).expect("QUAD casts");
            let fields = schema::collect_fields(
                quad.syntax(),
                quad.config().as_ref(),
                schema::QUAD_FIELDS,
                "`quad`",
                diagnostics,
            );
            SegmentDecl {
                kind: SegmentKind::Quad,
                name: quad.name_token().map(|t| t.text().to_string()),
                syntax: quad.syntax().clone(),
                at: None,
                to: fields.get("to").and_then(|f| f.value()),
                c: fields.get("c").and_then(|f| f.value()),
                c1: None,
                c2: None,
                center: None,
                rx: None,
                ry: None,
                large: false,
                sweep: None,
            }
        }
        SyntaxKind::CUBE => {
            let cube = ast::Cube::cast(node).expect("CUBE casts");
            let fields = schema::collect_fields(
                cube.syntax(),
                cube.config().as_ref(),
                schema::CUBE_FIELDS,
                "`cube`",
                diagnostics,
            );
            SegmentDecl {
                kind: SegmentKind::Cube,
                name: cube.name_token().map(|t| t.text().to_string()),
                syntax: cube.syntax().clone(),
                at: None,
                to: fields.get("to").and_then(|f| f.value()),
                c: None,
                c1: fields.get("c1").and_then(|f| f.value()),
                c2: fields.get("c2").and_then(|f| f.value()),
                center: None,
                rx: None,
                ry: None,
                large: false,
                sweep: None,
            }
        }
        SyntaxKind::ARC => {
            let arc = ast::Arc::cast(node).expect("ARC casts");
            let fields = schema::collect_fields(
                arc.syntax(),
                arc.config().as_ref(),
                schema::ARC_FIELDS,
                "`arc`",
                diagnostics,
            );
            check_arc_mode(&fields, arc.syntax(), diagnostics);
            let sweep = lower_sweep(&fields, diagnostics);
            let large = fields
                .get("large")
                .and_then(|f| f.value())
                .and_then(|e| const_eval::eval_const_bool(&e))
                .unwrap_or(false);
            SegmentDecl {
                kind: SegmentKind::Arc,
                name: arc.name_token().map(|t| t.text().to_string()),
                syntax: arc.syntax().clone(),
                at: None,
                to: fields.get("to").and_then(|f| f.value()),
                c: None,
                c1: None,
                c2: None,
                center: fields.get("center").and_then(|f| f.value()),
                rx: fields.get("rx").and_then(|f| f.value()),
                ry: fields.get("ry").and_then(|f| f.value()),
                large,
                sweep,
            }
        }
        _ => unreachable!("lower_segment called on a non-segment node"),
    }
}

fn lower_instance(
    instance_node: ast::Instance,
    declared_param_names: &IndexSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<InstanceDecl> {
    let Some(name_token) = instance_node.name_token() else {
        missing_name(instance_node.syntax(), "`instance`", diagnostics);
        return None;
    };
    let name = name_token.text().to_string();

    let fixed: Vec<&str> = schema::INSTANCE_FIXED_FIELDS
        .iter()
        .map(|f| f.name)
        .collect();
    let mut seen: IndexMap<String, ast::Field> = IndexMap::new();
    if let Some(config) = instance_node.config() {
        for field in config.fields() {
            let Some(token) = field.name_token() else {
                continue;
            };
            let field_name = token.text().to_string();
            let span: Range<usize> = token.text_range().into();
            let legal =
                fixed.contains(&field_name.as_str()) || declared_param_names.contains(&field_name);
            if !legal {
                let mut candidates: Vec<&str> = fixed.clone();
                candidates.extend(declared_param_names.iter().map(String::as_str));
                let diagnostic = Diagnostic::error(
                    codes::UNKNOWN_FIELD,
                    format!("`instance` has no field `{field_name}`"),
                    Label::new(span.clone(), "unknown field"),
                );
                let diagnostic = match mg_diag::suggest::nearest_match(&field_name, candidates) {
                    Some(s) => diagnostic.with_help(format!("did you mean `{s}`?")),
                    None => diagnostic,
                };
                diagnostics.push(diagnostic);
                continue;
            }
            if seen.contains_key(&field_name) {
                diagnostics.push(Diagnostic::error(
                    codes::DUPLICATE_DEFINITION,
                    format!("field `{field_name}` given more than once"),
                    Label::new(span, "duplicate field"),
                ));
                continue;
            }
            seen.insert(field_name, field);
        }
    }

    let overrides: IndexMap<String, ast::Expr> = declared_param_names
        .iter()
        .filter_map(|param_name| {
            let expr = seen.get(param_name)?.value()?;
            Some((param_name.clone(), expr))
        })
        .collect();

    let slant = seen.get("slant").and_then(|f| f.value());
    let glyphset = expect_ident_field(&seen, "glyphset", diagnostics).map(|(n, _)| n);
    let style_name = seen
        .get("styleName")
        .and_then(|f| f.value())
        .and_then(|e| string_literal_value(&e))
        .unwrap_or_else(|| name.clone());

    let weight_class = seen
        .get("weightClass")
        .and_then(|f| f.value())
        .and_then(|expr| lower_bounded_int(&expr, "weightClass", 1, 1000, diagnostics))
        .unwrap_or(400);
    let width_class = seen
        .get("widthClass")
        .and_then(|f| f.value())
        .and_then(|expr| lower_bounded_int(&expr, "widthClass", 1, 9, diagnostics))
        .unwrap_or(5);

    Some(InstanceDecl {
        name,
        syntax: instance_node.syntax().clone(),
        overrides,
        slant,
        glyphset,
        style_name,
        weight_class,
        width_class,
    })
}

fn lower_bounded_int(
    expr: &ast::Expr,
    field_name: &str,
    low: i64,
    high: i64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<i64> {
    let span: Range<usize> = expr.syntax().text_range().into();
    let Some(value) = const_eval::eval_const(expr, None) else {
        diagnostics.push(Diagnostic::error(
            codes::NON_CONSTANT_EXPRESSION,
            format!("`{field_name}` must be a constant expression"),
            Label::new(span, "not constant"),
        ));
        return None;
    };
    if value.fract() != 0.0 {
        diagnostics.push(Diagnostic::error(
            codes::NON_INTEGRAL_INT_FIELD,
            format!("`{field_name}` must be an exact integer, found {value}"),
            Label::new(span, "not an integer"),
        ));
        return None;
    }
    let int_value = value as i64;
    if !(low..=high).contains(&int_value) {
        diagnostics.push(Diagnostic::error(
            codes::VALUE_OUT_OF_RANGE,
            format!("`{field_name}` must be between {low} and {high}, found {int_value}"),
            Label::new(span, "out of range"),
        ));
        return None;
    }
    Some(int_value)
}

fn lower_group(group_node: ast::Group, diagnostics: &mut Vec<Diagnostic>) -> Option<GroupDecl> {
    let Some(name_token) = group_node.name_token() else {
        missing_name(group_node.syntax(), "`group`", diagnostics);
        return None;
    };
    let name = name_token.text().to_string();

    let fields = schema::collect_fields(
        group_node.syntax(),
        group_node.config().as_ref(),
        schema::GROUP_FIELDS,
        "`group`",
        diagnostics,
    );

    let glyphs_expr = fields.get("glyphs").and_then(|f| f.value());
    let elements: Vec<ast::Expr> = match &glyphs_expr {
        Some(ast::Expr::List(list)) => list.elements().collect(),
        Some(other) => vec![other.clone()],
        None => Vec::new(),
    };

    let mut glyphs = Vec::new();
    for element in &elements {
        match ident_text(element) {
            Some((name, _)) => glyphs.push(name),
            None => diagnostics.push(Diagnostic::error(
                codes::TYPE_MISMATCH,
                "`glyphs` must be a list of glyph names",
                Label::new(element.syntax().text_range().into(), "expected a name"),
            )),
        }
    }

    if glyphs.is_empty() {
        let span: Range<usize> = glyphs_expr
            .map(|e| e.syntax().text_range().into())
            .unwrap_or_else(|| group_node.syntax().text_range().into());
        diagnostics.push(Diagnostic::error(
            codes::EMPTY_GLYPH_LIST,
            "`group.glyphs` must not be empty",
            Label::new(span, "empty list"),
        ));
    }

    Some(GroupDecl {
        name,
        syntax: group_node.syntax().clone(),
        glyphs,
    })
}

fn lower_kern(kern_node: ast::Kern, diagnostics: &mut Vec<Diagnostic>) -> KernDecl {
    let fields = schema::collect_fields(
        kern_node.syntax(),
        kern_node.config().as_ref(),
        schema::KERN_FIELDS,
        "`kern`",
        diagnostics,
    );

    let left = expect_ident_field(&fields, "left", diagnostics);
    let right = expect_ident_field(&fields, "right", diagnostics);
    let by = fields.get("by").and_then(|f| f.value());

    KernDecl {
        syntax: kern_node.syntax().clone(),
        left_name: left.as_ref().map(|(name, _)| name.clone()),
        left_span: left.map(|(_, span)| span),
        right_name: right.as_ref().map(|(name, _)| name.clone()),
        right_span: right.map(|(_, span)| span),
        left: None,
        right: None,
        by,
    }
}

// ---------------------------------------------------------------------
// Pass 2: type-checking, and every check that needs the whole font

fn expect_type(ctx: &mut Ctx, expr: &ast::Expr, expected: Type, desc: &str) {
    let found = type_check::infer_expr(ctx, expr);
    if found == Type::Error || found == expected {
        return;
    }
    ctx.diagnostics.push(Diagnostic::error(
        codes::TYPE_MISMATCH,
        format!("{desc} must be `{expected}`, found `{found}`"),
        Label::new(
            expr.syntax().text_range().into(),
            format!("expected `{expected}`"),
        ),
    ));
}

fn check_min(ctx: &mut Ctx, expr: &ast::Expr, min: f64, desc: &str) {
    let font_em = ctx.hir.font.em.map(|v| v as f64);
    if let Some(value) = const_eval::eval_const(expr, font_em)
        && value < min
    {
        ctx.diagnostics.push(Diagnostic::error(
            codes::VALUE_OUT_OF_RANGE,
            format!("{desc} must be at least {min}, found {value}"),
            Label::new(expr.syntax().text_range().into(), "out of range"),
        ));
    }
}

fn typecheck_and_resolve(hir: &mut Hir, diagnostics: &mut Vec<Diagnostic>) {
    check_required_metrics(hir, diagnostics);
    typecheck_top_level(hir, diagnostics);

    let glyph_keys: Vec<GlyphKey> = hir.glyphs.keys().cloned().collect();
    for key in &glyph_keys {
        typecheck_glyph(hir, key, diagnostics);
    }

    check_glyph_and_group_namespace(hir, diagnostics);
    resolve_glyph_sets(hir, diagnostics);
    resolve_groups(hir, diagnostics);
    resolve_kerns(hir, diagnostics);
}

const REQUIRED_METRICS: &[&str] = &["baseline", "xHeight", "capHeight", "ascender", "descender"];

fn check_required_metrics(hir: &Hir, diagnostics: &mut Vec<Diagnostic>) {
    for name in REQUIRED_METRICS {
        if !hir.metrics.contains_key(*name) {
            diagnostics.push(Diagnostic::error(
                codes::MISSING_REQUIRED_METRIC,
                format!("missing required metric `{name}`"),
                Label::new(
                    hir.font.syntax.text_range().into(),
                    format!("no `{name}` declared"),
                ),
            ));
        }
    }

    if let Some(baseline) = hir.metrics.get("baseline")
        && let Some(y_expr) = &baseline.y
    {
        let font_em = hir.font.em.map(|v| v as f64);
        if let Some(value) = const_eval::eval_const(y_expr, font_em)
            && value != 0.0
        {
            diagnostics.push(Diagnostic::error(
                codes::BASELINE_NOT_ZERO,
                format!("`baseline.y` must be 0, found {value}"),
                Label::new(y_expr.syntax().text_range().into(), "must be 0"),
            ));
        }
    }
}

fn typecheck_top_level(hir: &mut Hir, diagnostics: &mut Vec<Diagnostic>) {
    let metric_exprs: Vec<(Option<ast::Expr>, Option<ast::Expr>)> = hir
        .metrics
        .values()
        .map(|m| (m.y.clone(), m.overshoot.clone()))
        .collect();
    for (y, overshoot) in metric_exprs {
        let mut ctx = Ctx::new(hir, diagnostics);
        if let Some(y) = &y {
            expect_type(&mut ctx, y, Type::Num, "a metric's `y`");
        }
        if let Some(overshoot) = &overshoot {
            expect_type(&mut ctx, overshoot, Type::Num, "`overshoot`");
            check_min(&mut ctx, overshoot, 0.0, "`overshoot`");
        }
    }

    let let_names: Vec<String> = hir.lets.keys().cloned().collect();
    for name in &let_names {
        let mut ctx = Ctx::new(hir, diagnostics);
        type_check::top_level_type(&mut ctx, name);
    }
}

fn typecheck_glyph(hir: &mut Hir, key: &GlyphKey, diagnostics: &mut Vec<Diagnostic>) {
    let Some(glyph) = hir.glyphs.get(key) else {
        return;
    };
    let advance = glyph.advance.clone();
    let let_names: Vec<String> = glyph.lets.keys().cloned().collect();
    let anchor_ats: Vec<Option<ast::Expr>> = glyph.anchors.values().map(|a| a.at.clone()).collect();
    let component_fields: Vec<ComponentExprs> = glyph
        .components
        .iter()
        .map(|c| ComponentExprs {
            offset: c.offset.clone(),
            transform: c.transform.clone(),
            glyph_ref: c.glyph.clone(),
            span: c.syntax.text_range().into(),
        })
        .collect();
    let path_data: Vec<(Option<ast::Expr>, Vec<SegmentExprs>)> = glyph
        .paths
        .iter()
        .map(|p| {
            let segs = p
                .segments
                .iter()
                .map(|s| SegmentExprs {
                    at: s.at.clone(),
                    to: s.to.clone(),
                    c: s.c.clone(),
                    c1: s.c1.clone(),
                    c2: s.c2.clone(),
                    center: s.center.clone(),
                    rx: s.rx.clone(),
                    ry: s.ry.clone(),
                })
                .collect();
            (p.stroke.clone(), segs)
        })
        .collect();

    for name in &let_names {
        let mut ctx = Ctx::new(hir, diagnostics);
        ctx.current_glyph = Some(key.clone());
        type_check::glyph_local_type(&mut ctx, key, name);
    }

    if let Some(advance) = &advance {
        let mut ctx = Ctx::new(hir, diagnostics);
        ctx.current_glyph = Some(key.clone());
        expect_type(&mut ctx, advance, Type::Num, "`advance`");
    }

    for at in anchor_ats.iter().flatten() {
        let mut ctx = Ctx::new(hir, diagnostics);
        ctx.current_glyph = Some(key.clone());
        expect_type(&mut ctx, at, Type::Pair, "an anchor's `at`");
    }

    for comp in &component_fields {
        let mut ctx = Ctx::new(hir, diagnostics);
        ctx.current_glyph = Some(key.clone());
        if let Some(offset) = &comp.offset {
            expect_type(&mut ctx, offset, Type::Pair, "`offset`");
        }
        if let Some(transform) = &comp.transform {
            expect_type(&mut ctx, transform, Type::Transform, "`transform`");
        }
        if let Some(name) = &comp.glyph_ref
            && !ctx.hir.glyphs.contains_key(&(name.clone(), None))
        {
            let candidates: Vec<String> = ctx
                .hir
                .glyphs
                .keys()
                .filter(|(_, set)| set.is_none())
                .map(|(n, _)| n.clone())
                .collect();
            let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            ctx.diagnostics.push(resolve::unresolved_name(
                name,
                comp.span.clone(),
                "the glyph namespace",
                refs.into_iter(),
            ));
        }
    }

    for (stroke, segments) in &path_data {
        let mut ctx = Ctx::new(hir, diagnostics);
        ctx.current_glyph = Some(key.clone());
        if let Some(stroke) = stroke {
            expect_type(&mut ctx, stroke, Type::Num, "`stroke`");
            check_min_exclusive(&mut ctx, stroke, 0.0, "`stroke`");
        }
        for seg in segments {
            if let Some(at) = &seg.at {
                expect_type(&mut ctx, at, Type::Pair, "`at`");
            }
            if let Some(to) = &seg.to {
                expect_type(&mut ctx, to, Type::Pair, "`to`");
            }
            if let Some(c) = &seg.c {
                expect_type(&mut ctx, c, Type::Pair, "`c`");
            }
            if let Some(c1) = &seg.c1 {
                expect_type(&mut ctx, c1, Type::Pair, "`c1`");
            }
            if let Some(c2) = &seg.c2 {
                expect_type(&mut ctx, c2, Type::Pair, "`c2`");
            }
            if let Some(center) = &seg.center {
                expect_type(&mut ctx, center, Type::Pair, "`center`");
            }
            if let Some(rx) = &seg.rx {
                expect_type(&mut ctx, rx, Type::Num, "`rx`");
                check_min_exclusive(&mut ctx, rx, 0.0, "`rx`");
            }
            if let Some(ry) = &seg.ry {
                expect_type(&mut ctx, ry, Type::Num, "`ry`");
                check_min_exclusive(&mut ctx, ry, 0.0, "`ry`");
            }
        }
    }
}

struct ComponentExprs {
    offset: Option<ast::Expr>,
    transform: Option<ast::Expr>,
    glyph_ref: Option<String>,
    span: Range<usize>,
}

struct SegmentExprs {
    at: Option<ast::Expr>,
    to: Option<ast::Expr>,
    c: Option<ast::Expr>,
    c1: Option<ast::Expr>,
    c2: Option<ast::Expr>,
    center: Option<ast::Expr>,
    rx: Option<ast::Expr>,
    ry: Option<ast::Expr>,
}

fn check_min_exclusive(ctx: &mut Ctx, expr: &ast::Expr, min: f64, desc: &str) {
    let font_em = ctx.hir.font.em.map(|v| v as f64);
    if let Some(value) = const_eval::eval_const(expr, font_em)
        && value <= min
    {
        ctx.diagnostics.push(Diagnostic::error(
            codes::VALUE_OUT_OF_RANGE,
            format!("{desc} must be greater than {min}, found {value}"),
            Label::new(expr.syntax().text_range().into(), "out of range"),
        ));
    }
}

fn check_glyph_and_group_namespace(hir: &Hir, diagnostics: &mut Vec<Diagnostic>) {
    let mut distinct_names: IndexMap<String, Range<usize>> = IndexMap::new();
    for ((name, set), decl) in &hir.glyphs {
        if name.len() > 63 {
            diagnostics.push(Diagnostic::error(
                codes::GLYPH_NAME_TOO_LONG,
                format!(
                    "glyph name `{name}` is {} bytes, over the 63-byte limit",
                    name.len()
                ),
                Label::new(decl.syntax.text_range().into(), "name too long"),
            ));
        }

        match set {
            None => {
                distinct_names.insert(name.clone(), decl.syntax.text_range().into());
            }
            Some(set_name) => {
                if !hir.glyphs.contains_key(&(name.clone(), None)) {
                    diagnostics.push(Diagnostic::error(
                        codes::ALTERNATE_WITHOUT_DEFAULT,
                        format!("`{name}` in glyph set `{set_name}` has no default-set glyph"),
                        Label::new(decl.syntax.text_range().into(), "no default glyph"),
                    ));
                }
                if let Some(codepoint_expr) = &decl.codepoint_expr {
                    diagnostics.push(Diagnostic::error(
                        codes::ALTERNATE_WITH_CODEPOINT,
                        format!("alternate glyph `{name}` in glyph set `{set_name}` may not declare `codepoint`"),
                        Label::new(codepoint_expr.syntax().text_range().into(), "illegal here"),
                    ));
                }
            }
        }
    }

    for (group_name, group) in &hir.groups {
        if let Some(glyph_span) = distinct_names.get(group_name) {
            diagnostics.push(
                Diagnostic::error(
                    codes::DUPLICATE_DEFINITION,
                    format!("`{group_name}` names both a glyph and a group"),
                    Label::new(group.syntax.text_range().into(), "group defined here"),
                )
                .with_secondary(Label::new(glyph_span.clone(), "glyph defined here")),
            );
        } else {
            distinct_names.insert(group_name.clone(), group.syntax.text_range().into());
        }
    }
}

fn resolve_glyph_sets(hir: &mut Hir, diagnostics: &mut Vec<Diagnostic>) {
    let glyph_sets: IndexSet<String> = hir
        .glyphs
        .keys()
        .filter_map(|(_, set)| set.clone())
        .collect();

    let instance_data: Vec<(String, Option<String>, Range<usize>)> = hir
        .instances
        .values()
        .filter_map(|inst| {
            let set = inst.glyphset.clone()?;
            Some((
                inst.name.clone(),
                Some(set),
                inst.syntax.text_range().into(),
            ))
        })
        .collect();

    for (_, set, span) in instance_data {
        let Some(set) = set else { continue };
        if !glyph_sets.contains(&set) {
            let refs: Vec<&str> = glyph_sets.iter().map(String::as_str).collect();
            diagnostics.push(resolve::unresolved_name(
                &set,
                span,
                "the glyph-set namespace",
                refs.into_iter(),
            ));
        }
    }

    let overrides: Vec<(String, Vec<(String, ast::Expr)>)> = hir
        .instances
        .values()
        .map(|inst| {
            (
                inst.name.clone(),
                inst.overrides.clone().into_iter().collect(),
            )
        })
        .collect();
    for (_, param_overrides) in overrides {
        for (param_name, expr) in param_overrides {
            let mut ctx = Ctx::new(hir, diagnostics);
            expect_type(&mut ctx, &expr, Type::Num, "an instance param override");
            let font_em = ctx.hir.font.em.map(|v| v as f64);
            let Some(value) = const_eval::eval_const(&expr, font_em) else {
                ctx.diagnostics.push(Diagnostic::error(
                    codes::NON_CONSTANT_EXPRESSION,
                    "an instance override must be a constant expression",
                    Label::new(expr.syntax().text_range().into(), "not constant"),
                ));
                continue;
            };
            if let Some(param) = ctx.hir.params.get(&param_name)
                && let Some((low, high)) = param.range
                && (value < low || value > high)
            {
                ctx.diagnostics.push(Diagnostic::error(
                    codes::VALUE_OUT_OF_RANGE,
                    format!("override for `{param_name}` ({value}) lies outside its range ({low}..{high})"),
                    Label::new(expr.syntax().text_range().into(), "outside range"),
                ));
            }
        }
    }

    let slants: Vec<Option<ast::Expr>> = hir.instances.values().map(|i| i.slant.clone()).collect();
    for slant in slants.into_iter().flatten() {
        let mut ctx = Ctx::new(hir, diagnostics);
        expect_type(&mut ctx, &slant, Type::Num, "`slant`");
        let font_em = ctx.hir.font.em.map(|v| v as f64);
        if const_eval::eval_const(&slant, font_em).is_none() {
            ctx.diagnostics.push(Diagnostic::error(
                codes::NON_CONSTANT_EXPRESSION,
                "`slant` must be a constant expression",
                Label::new(slant.syntax().text_range().into(), "not constant"),
            ));
        }
    }
}

fn resolve_groups(hir: &Hir, diagnostics: &mut Vec<Diagnostic>) {
    for group in hir.groups.values() {
        for glyph_name in &group.glyphs {
            if !hir.glyphs.contains_key(&(glyph_name.clone(), None)) {
                let candidates: Vec<String> = hir
                    .glyphs
                    .keys()
                    .filter(|(_, set)| set.is_none())
                    .map(|(n, _)| n.clone())
                    .collect();
                let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
                diagnostics.push(resolve::unresolved_name(
                    glyph_name,
                    group.syntax.text_range().into(),
                    "the glyph namespace",
                    refs.into_iter(),
                ));
            }
        }
    }
}

fn resolve_side(
    hir: &Hir,
    name: &Option<String>,
    span: &Option<Range<usize>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<KernSide> {
    let name = name.as_ref()?;
    let span = span.clone().unwrap_or(0..0);
    if hir.glyphs.contains_key(&(name.clone(), None)) {
        return Some(KernSide::Glyph(name.clone()));
    }
    if hir.groups.contains_key(name) {
        return Some(KernSide::Group(name.clone()));
    }
    let mut candidates: Vec<String> = hir
        .glyphs
        .keys()
        .filter(|(_, set)| set.is_none())
        .map(|(n, _)| n.clone())
        .collect();
    candidates.extend(hir.groups.keys().cloned());
    let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    diagnostics.push(resolve::unresolved_name(
        name,
        span,
        "the glyph or group namespace",
        refs.into_iter(),
    ));
    None
}

fn resolve_kerns(hir: &mut Hir, diagnostics: &mut Vec<Diagnostic>) {
    for i in 0..hir.kerns.len() {
        let (left_name, left_span, right_name, right_span, by) = {
            let k = &hir.kerns[i];
            (
                k.left_name.clone(),
                k.left_span.clone(),
                k.right_name.clone(),
                k.right_span.clone(),
                k.by.clone(),
            )
        };
        let left = resolve_side(hir, &left_name, &left_span, diagnostics);
        let right = resolve_side(hir, &right_name, &right_span, diagnostics);
        if let Some(by) = &by {
            let mut ctx = Ctx::new(hir, diagnostics);
            expect_type(&mut ctx, by, Type::Num, "`by`");
        }
        let k = &mut hir.kerns[i];
        k.left = left;
        k.right = right;
    }

    check_kern_group_overlap(hir, diagnostics);
    check_duplicate_kern_pairs(hir, diagnostics);
}

fn check_kern_group_overlap(hir: &Hir, diagnostics: &mut Vec<Diagnostic>) {
    for side in [Side::Left, Side::Right] {
        let groups_used: Vec<&str> = hir
            .kerns
            .iter()
            .filter_map(|k| match side.pick(k) {
                Some(KernSide::Group(name)) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        for i in 0..groups_used.len() {
            for j in (i + 1)..groups_used.len() {
                let (g1, g2) = (groups_used[i], groups_used[j]);
                if g1 == g2 {
                    continue;
                }
                let (Some(group1), Some(group2)) = (hir.groups.get(g1), hir.groups.get(g2)) else {
                    continue;
                };
                if let Some(shared) = group1.glyphs.iter().find(|g| group2.glyphs.contains(g)) {
                    diagnostics.push(Diagnostic::error(
                        codes::KERN_GROUP_OVERLAP,
                        format!(
                            "`{shared}` belongs to both `{g1}` and `{g2}`, which both appear as kern {}",
                            side.desc()
                        ),
                        Label::new(group1.syntax.text_range().into(), "first group here"),
                    ));
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn pick<'a>(&self, kern: &'a KernDecl) -> &'a Option<KernSide> {
        match self {
            Side::Left => &kern.left,
            Side::Right => &kern.right,
        }
    }

    fn desc(&self) -> &'static str {
        match self {
            Side::Left => "`left`",
            Side::Right => "`right`",
        }
    }
}

type KernSideKey<'a> = (&'static str, &'a str);

fn kern_side_key(side: &KernSide) -> KernSideKey<'_> {
    match side {
        KernSide::Glyph(name) => ("glyph", name.as_str()),
        KernSide::Group(name) => ("group", name.as_str()),
    }
}

fn check_duplicate_kern_pairs(hir: &Hir, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: Vec<(KernSideKey, KernSideKey)> = Vec::new();
    for kern in &hir.kerns {
        let (Some(left), Some(right)) = (&kern.left, &kern.right) else {
            continue;
        };
        let key = (kern_side_key(left), kern_side_key(right));
        if seen.contains(&key) {
            diagnostics.push(Diagnostic::error(
                codes::DUPLICATE_KERN_PAIR,
                format!("duplicate kern pair (`{}`, `{}`)", key.0.1, key.1.1),
                Label::new(kern.syntax.text_range().into(), "duplicate pair"),
            ));
        } else {
            seen.push(key);
        }
    }
}
