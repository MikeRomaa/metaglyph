//! `textDocument/completion` (plan 4, L2), chosen by the cursor's CST
//! context. Field names, types, defaults, and enum sets come from the
//! M2 schema table (`mg_hir::schema`), and functions, members, and
//! constants from `mg_hir::types`; nothing here keeps a list of its own.
//!
//! The parser leaves an empty `FIELD` or `MAP_ENTRY` at every slot it
//! was expecting, so the context is read off the last real token before
//! the cursor (skipping the name being typed) and that token's parent:
//!
//! | Before the cursor | Offers |
//! |---|---|
//! | `.` | the receiver's members |
//! | `(` or `,` in a config | the block's fields not yet present |
//! | `:` in a field | that field's values |
//! | `:` in a `joinAt` entry | join kinds |
//! | `{` or `,` in a `joinAt` map | the path's segment names |
//! | `[` or `,` in `group (glyphs: …)` | glyph names |
//! | a statement boundary | the declaration kinds legal there |
//! | anything else in an expression | names, constants, namespaces, functions |
//!
//! Inside a string literal, an enum field offers its bare values.

use lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};
use mg_hir::schema::{self, FieldSchema};
use mg_hir::types::{self, Type};
use mg_syntax::ast::{self, AstNode};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::index::Index;
use crate::types::{NameTypes, glyphs_member_target};

pub struct Ctx<'a> {
    pub root: &'a SyntaxNode,
    pub index: &'a Index,
    pub types: &'a NameTypes,
    pub offset: usize,
    /// Whether the client accepts snippet syntax in `insertText`.
    pub snippets: bool,
}

const TOP_LEVEL_KINDS: &[&str] = &[
    "font", "param", "metric", "let", "glyph", "instance", "group", "kern",
];
const GLYPH_BODY_KINDS: &[&str] = &["let", "path", "anchor", "component"];
const PATH_BODY_KINDS: &[&str] = &["start", "line", "quad", "cube", "arc", "close"];
const CONSTANTS: &[&str] = &["up", "down", "left", "right", "identity"];
const FONT_MEMBERS: &[&str] = &["name", "em", "version", "designer", "foundry", "license"];
const GLYPH_MEMBERS: &[&str] = &["name", "codepoints", "advance", "bbox"];
const INSTANCE_MEMBERS: &[&str] = &["name", "slant"];
const MATH_MEMBERS: &[&str] = &["pi", "tau", "e"];

fn is_trivia(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
}

/// A name-shaped token: an identifier or a keyword being typed.
fn is_wordlike(token: &SyntaxToken) -> bool {
    token
        .text()
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// The last non-trivia token ending at or before `offset`.
///
/// Searched by offset rather than with `prev_token`: the parser leaves
/// empty placeholder nodes (a `FIELD` with no tokens) at the very slots
/// completion cares about, and rowan's `prev_token` stops at an empty
/// sibling instead of stepping past it.
fn previous_real(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let mut offset = offset as u32;
    while offset > 0 {
        let token = root.token_at_offset(offset.into()).left_biased()?;
        let start = u32::from(token.text_range().start());
        if start >= offset {
            // Only a zero-width token (`EOF`) can start here; nothing
            // real precedes it at this offset.
            return None;
        }
        if !is_trivia(token.kind()) {
            return Some(token);
        }
        offset = start;
    }
    None
}

fn item(
    label: &str,
    kind: CompletionItemKind,
    detail: impl Into<Option<String>>,
) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: detail.into(),
        ..Default::default()
    }
}

fn markdown(value: String) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

/// The markdown documentation of a schema field: its line of meaning,
/// then required or default, then legal values.
pub fn field_documentation(field: &FieldSchema) -> String {
    let mut doc = field.doc.to_string();
    if field.required {
        doc.push_str("\n\nRequired.");
    } else if let Some(default) = field.default {
        doc.push_str(&format!("\n\nOptional; default `{default}`."));
    } else {
        doc.push_str("\n\nOptional.");
    }
    if !field.values.is_empty() {
        let values: Vec<String> = field.values.iter().map(|v| format!("`\"{v}\"`")).collect();
        doc.push_str(&format!(" One of {}.", values.join(", ")));
    }
    doc
}

/// `name(p: pair, len: num, θ: num) → pair` for each of a function's
/// signatures, one per line.
pub fn function_signatures(name: &str) -> Option<String> {
    let doc = types::function_doc(name)?;
    let sigs = types::lookup_function(name)?;
    let lines: Vec<String> = sigs
        .iter()
        .map(|sig| {
            // Past the first signature, placeholder names only fit when
            // the arity matches; otherwise the types stand alone.
            let params: Vec<String> = sig
                .params
                .iter()
                .enumerate()
                .map(|(i, ty)| match doc.params.get(i) {
                    Some(name) if sig.params.len() == doc.params.len() => format!("{name}: {ty}"),
                    _ => ty.to_string(),
                })
                .collect();
            format!("{name}({}) → {}", params.join(", "), sig.ret)
        })
        .collect();
    Some(lines.join("\n"))
}

pub fn complete(ctx: &Ctx) -> Vec<CompletionItem> {
    let offset = ctx.offset.min(u32::MAX as usize) as u32;
    let Some(left) = ctx.root.token_at_offset(offset.into()).left_biased() else {
        return statement_keywords(&ctx.root.clone());
    };
    let start = usize::from(left.text_range().start());

    match left.kind() {
        SyntaxKind::COMMENT if start < ctx.offset => return Vec::new(),
        SyntaxKind::STRING if start < ctx.offset && !closed_before(&left, ctx.offset) => {
            return string_values(&left);
        }
        _ => {}
    }

    let partial = (is_wordlike(&left) && start < ctx.offset).then(|| left.clone());
    let before = match &partial {
        Some(p) => previous_real(ctx.root, p.text_range().start().into()),
        None => previous_real(ctx.root, ctx.offset),
    };

    if let Some(p) = &partial
        && p.parent().is_some_and(|n| n.kind() == SyntaxKind::ERROR)
    {
        let container = p
            .parent()
            .and_then(|e| e.parent())
            .unwrap_or(ctx.root.clone());
        return statement_keywords(&container);
    }

    let Some(before) = before else {
        return statement_keywords(&ctx.root.clone());
    };
    let Some(parent) = before.parent() else {
        return Vec::new();
    };

    match (before.kind(), parent.kind()) {
        (SyntaxKind::DOT, SyntaxKind::MEMBER_EXPR) => ast::MemberExpr::cast(parent)
            .and_then(|m| m.receiver())
            .map(|receiver| members(ctx, &receiver))
            .unwrap_or_default(),
        (SyntaxKind::L_PAREN | SyntaxKind::COMMA, SyntaxKind::CONFIG) => {
            field_names(ctx, &parent, partial.as_ref())
        }
        (SyntaxKind::COLON, SyntaxKind::FIELD) => field_values(ctx, &parent),
        (SyntaxKind::COLON, SyntaxKind::MAP_ENTRY) => quoted(schema::JOIN_VALUES),
        (SyntaxKind::L_BRACE | SyntaxKind::COMMA, SyntaxKind::MAP_EXPR) => {
            join_at_keys(ctx, &parent)
        }
        (SyntaxKind::L_BRACKET | SyntaxKind::COMMA, SyntaxKind::LIST_EXPR)
            if field_of(&parent)
                .is_some_and(|(block, name)| block == SyntaxKind::GROUP && name == "glyphs") =>
        {
            glyph_names(ctx, false)
        }
        (SyntaxKind::SEMICOLON, _) => {
            statement_keywords(&parent.parent().unwrap_or(ctx.root.clone()))
        }
        (SyntaxKind::L_BRACE, SyntaxKind::BODY) => statement_keywords(&parent),
        (SyntaxKind::R_BRACE, SyntaxKind::BODY) | (SyntaxKind::R_PAREN, SyntaxKind::CONFIG) => {
            // The declaration just closed; the next one sits beside it.
            let container = parent
                .parent()
                .and_then(|decl| decl.parent())
                .unwrap_or(ctx.root.clone());
            statement_keywords(&container)
        }
        _ => expression(ctx),
    }
}

/// Whether `string`, a `STRING` token, has its closing quote before
/// `offset`.
fn closed_before(string: &SyntaxToken, offset: usize) -> bool {
    let text = string.text();
    let end = usize::from(string.text_range().end());
    text.len() >= 2 && text.ends_with('"') && !text.ends_with("\\\"") && offset >= end
}

/// The kind of block a field belongs to, and the field's name.
fn field_of(node: &SyntaxNode) -> Option<(SyntaxKind, String)> {
    let field = node.ancestors().find_map(ast::Field::cast)?;
    let block = field.syntax().parent()?.parent()?.kind();
    Some((block, field.name_token()?.text().to_string()))
}

fn quoted(values: &[&str]) -> Vec<CompletionItem> {
    values
        .iter()
        .map(|v| CompletionItem {
            filter_text: Some(v.to_string()),
            insert_text: Some(format!("\"{v}\"")),
            ..item(&format!("\"{v}\""), CompletionItemKind::ENUM_MEMBER, None)
        })
        .collect()
}

/// Inside `"…"`: the bare legal values of the enum the string belongs to.
fn string_values(string: &SyntaxToken) -> Vec<CompletionItem> {
    let Some(literal) = string.parent() else {
        return Vec::new();
    };
    let values: &[&str] = match literal.parent().map(|p| p.kind()) {
        Some(SyntaxKind::MAP_ENTRY) => schema::JOIN_VALUES,
        Some(SyntaxKind::FIELD | SyntaxKind::TUPLE_EXPR) => {
            let Some((block, name)) = field_of(&literal) else {
                return Vec::new();
            };
            schema::fields_for(block)
                .and_then(|table| table.iter().find(|f| f.name == name))
                .map_or(&[][..], |f| f.values)
        }
        _ => &[],
    };
    values
        .iter()
        .map(|v| item(v, CompletionItemKind::ENUM_MEMBER, None))
        .collect()
}

fn statement_keywords(container: &SyntaxNode) -> Vec<CompletionItem> {
    let kinds = match container.kind() {
        SyntaxKind::BODY => match container.parent().map(|p| p.kind()) {
            Some(SyntaxKind::GLYPH) => GLYPH_BODY_KINDS,
            Some(SyntaxKind::PATH) => PATH_BODY_KINDS,
            _ => &[],
        },
        _ => TOP_LEVEL_KINDS,
    };
    kinds
        .iter()
        .map(|k| item(k, CompletionItemKind::KEYWORD, None))
        .collect()
}

fn field_names(
    ctx: &Ctx,
    config: &SyntaxNode,
    partial: Option<&SyntaxToken>,
) -> Vec<CompletionItem> {
    let Some(block) = config.parent() else {
        return Vec::new();
    };
    let Some(table) = schema::fields_for(block.kind()) else {
        return Vec::new();
    };
    let present: Vec<String> = config
        .children()
        .filter_map(ast::Field::cast)
        .filter_map(|f| f.name_token())
        .filter(|t| Some(t) != partial)
        .map(|t| t.text().to_string())
        .collect();
    let has = |name: &str| present.iter().any(|p| p == name);

    let insert = |name: &str| {
        if ctx.snippets {
            (format!("{name}: $0"), InsertTextFormat::SNIPPET)
        } else {
            (format!("{name}: "), InsertTextFormat::PLAIN_TEXT)
        }
    };

    let mut items: Vec<CompletionItem> = table
        .iter()
        .filter(|f| !has(f.name) && !f.mutex.iter().any(|m| has(m)))
        .map(|f| {
            let (text, format) = insert(f.name);
            CompletionItem {
                documentation: Some(markdown(field_documentation(f))),
                insert_text: Some(text),
                insert_text_format: Some(format),
                ..item(f.name, CompletionItemKind::FIELD, Some(f.ty.to_string()))
            }
        })
        .collect();

    // An instance also overrides any param (spec §5.6).
    if block.kind() == SyntaxKind::INSTANCE {
        for param in &ctx.index.params {
            if has(&param.name) {
                continue;
            }
            let (text, format) = insert(&param.name);
            items.push(CompletionItem {
                insert_text: Some(text),
                insert_text_format: Some(format),
                ..item(
                    &param.name,
                    CompletionItemKind::FIELD,
                    Some("num — overrides the param".to_string()),
                )
            });
        }
    }
    items
}

fn field_values(ctx: &Ctx, field: &SyntaxNode) -> Vec<CompletionItem> {
    let Some((block, name)) = field_of(field) else {
        return expression(ctx);
    };
    match (block, name.as_str()) {
        (SyntaxKind::COMPONENT, "glyph") => {
            // A glyph can't be its own component: that is a cycle.
            let this = ctx
                .index
                .glyph_at(ctx.offset)
                .map(|g| ctx.index.glyphs[g].decl.name.clone());
            let mut items = glyph_names(ctx, false);
            items.retain(|i| Some(&i.label) != this.as_ref());
            items
        }
        (SyntaxKind::KERN, "left" | "right") => glyph_names(ctx, true),
        (SyntaxKind::GLYPH | SyntaxKind::INSTANCE, "glyphset") => {
            let mut sets: Vec<&str> = ctx
                .index
                .glyphs
                .iter()
                .filter_map(|g| g.glyphset.as_deref())
                .collect();
            sets.dedup();
            sets.iter()
                .map(|s| item(s, CompletionItemKind::ENUM, Some("glyph set".to_string())))
                .collect()
        }
        (SyntaxKind::PATH, "joinAt") => Vec::new(),
        _ => {
            let values = schema::fields_for(block)
                .and_then(|table| table.iter().find(|f| f.name == name))
                .map_or(&[][..], |f| f.values);
            if values.is_empty() {
                expression(ctx)
            } else {
                quoted(values)
            }
        }
    }
}

/// Default-set glyph names, and group names too when `groups`.
fn glyph_names(ctx: &Ctx, groups: bool) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = ctx
        .index
        .glyphs
        .iter()
        .filter(|g| g.glyphset.is_none())
        .map(|g| {
            item(
                &g.decl.name,
                CompletionItemKind::CLASS,
                Some("glyph".to_string()),
            )
        })
        .collect();
    if groups {
        items.extend(
            ctx.index
                .groups
                .iter()
                .map(|g| item(&g.name, CompletionItemKind::ENUM, Some("group".to_string()))),
        );
    }
    items
}

/// A `joinAt` map's keys: the enclosing path's segment names.
fn join_at_keys(ctx: &Ctx, map: &SyntaxNode) -> Vec<CompletionItem> {
    if field_of(map).is_none_or(|(_, name)| name != "joinAt") {
        return Vec::new();
    }
    let Some(glyph) = ctx.index.glyph_at(ctx.offset) else {
        return Vec::new();
    };
    let Some(path) = ctx.index.path_at(glyph, ctx.offset) else {
        return Vec::new();
    };
    ctx.index.glyphs[glyph].paths[path]
        .segments
        .iter()
        .map(|s| {
            item(
                &s.name,
                CompletionItemKind::FIELD,
                Some("segment".to_string()),
            )
        })
        .collect()
}

fn members(ctx: &Ctx, receiver: &ast::Expr) -> Vec<CompletionItem> {
    let glyph = ctx
        .index
        .glyph_at(ctx.offset)
        .map(|g| ctx.index.glyphs[g].decl.name.clone());
    let typed = |names: &[&str], ty: &dyn Fn(&str) -> Option<Type>| -> Vec<CompletionItem> {
        names
            .iter()
            .map(|n| {
                item(
                    n,
                    CompletionItemKind::PROPERTY,
                    ty(n).map(|t| t.to_string()),
                )
            })
            .collect()
    };

    if let ast::Expr::Ident(root) = receiver
        && let Some(token) = root.token()
    {
        match token.text() {
            "glyphs" => return glyph_names(ctx, false),
            "font" => return typed(FONT_MEMBERS, &types::font_member),
            "glyph" => return typed(GLYPH_MEMBERS, &types::glyph_member),
            "instance" => return typed(INSTANCE_MEMBERS, &types::instance_member),
            "math" => return typed(MATH_MEMBERS, &types::math_member),
            _ => {}
        }
    }

    if let Some(target) = glyphs_member_target(receiver) {
        let mut items = vec![
            item(
                "advance",
                CompletionItemKind::PROPERTY,
                Some("num".to_string()),
            ),
            item(
                "bbox",
                CompletionItemKind::PROPERTY,
                Some("rect".to_string()),
            ),
        ];
        if let Some(g) = ctx.index.default_glyph(&target) {
            items.extend(ctx.index.glyphs[g].anchors.iter().map(|a| {
                item(
                    &a.name,
                    CompletionItemKind::PROPERTY,
                    Some("anchor: pair".to_string()),
                )
            }));
            // Its named paths (spec §5.10), for path components and
            // path queries.
            items.extend(
                ctx.index.glyphs[g]
                    .paths
                    .iter()
                    .filter_map(|p| p.decl.as_ref())
                    .map(|d| item(&d.name, CompletionItemKind::PROPERTY, Some("path".to_string()))),
            );
        }
        return items;
    }

    let Some(ty) = ctx.types.expr(glyph.as_deref(), receiver) else {
        return Vec::new();
    };
    types::member_names(&ty)
        .iter()
        .map(|m| {
            item(
                m,
                CompletionItemKind::PROPERTY,
                types::member_type(&ty, m).map(|t| t.to_string()),
            )
        })
        .collect()
}

fn expression(ctx: &Ctx) -> Vec<CompletionItem> {
    let glyph = ctx.index.glyph_at(ctx.offset);
    let glyph_name = glyph.map(|g| ctx.index.glyphs[g].decl.name.as_str());
    let type_of = |name: &str| ctx.types.name(glyph_name, name).map(|t| t.to_string());
    let mut items = Vec::new();

    if let Some(g) = glyph {
        let entry = &ctx.index.glyphs[g];
        items.extend(
            entry
                .lets
                .iter()
                .map(|d| item(&d.name, CompletionItemKind::VARIABLE, type_of(&d.name))),
        );
        items.extend(entry.anchors.iter().map(|d| {
            item(
                &d.name,
                CompletionItemKind::PROPERTY,
                Some("pair".to_string()),
            )
        }));
        items.extend(entry.paths.iter().filter_map(|p| p.decl.as_ref()).map(|d| {
            item(
                &d.name,
                CompletionItemKind::FUNCTION,
                Some("path".to_string()),
            )
        }));
    }
    items.extend(ctx.index.params.iter().map(|d| {
        item(
            &d.name,
            CompletionItemKind::CONSTANT,
            Some("num".to_string()),
        )
    }));
    items.extend(ctx.index.metrics.iter().map(|d| {
        item(
            &d.name,
            CompletionItemKind::CONSTANT,
            Some("zone".to_string()),
        )
    }));
    items.extend(
        ctx.index
            .lets
            .iter()
            .map(|d| item(&d.name, CompletionItemKind::VARIABLE, type_of(&d.name))),
    );

    items.extend(CONSTANTS.iter().map(|c| {
        item(
            c,
            CompletionItemKind::CONSTANT,
            types::builtin_constant(c).map(|t| t.to_string()),
        )
    }));
    items.extend(
        ["true", "false"].map(|b| item(b, CompletionItemKind::KEYWORD, Some("bool".to_string()))),
    );

    let mut roots = vec!["math", "font", "glyphs", "instance"];
    if glyph.is_some() {
        roots.push("glyph");
    }
    items.extend(
        roots
            .iter()
            .map(|r| item(r, CompletionItemKind::MODULE, None)),
    );

    for name in types::FUNCTION_NAMES {
        let Some(doc) = types::function_doc(name) else {
            continue;
        };
        let (text, format) = if ctx.snippets {
            let placeholders: Vec<String> = doc
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| format!("${{{}:{p}}}", i + 1))
                .collect();
            (
                format!("{name}({})", placeholders.join(", ")),
                InsertTextFormat::SNIPPET,
            )
        } else {
            (format!("{name}("), InsertTextFormat::PLAIN_TEXT)
        };
        items.push(CompletionItem {
            documentation: Some(markdown(doc.doc.to_string())),
            insert_text: Some(text),
            insert_text_format: Some(format),
            ..item(
                name,
                CompletionItemKind::FUNCTION,
                function_signatures(name),
            )
        });
    }
    items
}
