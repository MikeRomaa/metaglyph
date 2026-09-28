//! `textDocument/hover`, static half (plan 4, L2). What a token means
//! without evaluating anything:
//!
//! - a name: its declaration kind, its type, and its declaration line
//! - a function: its §5.9 signatures and one-line description
//! - a field name: its type, required or default, legal values, and meaning
//! - a namespace or typed member: its type
//! - a suffixed number: its converted internal value (`152deg` → radians)
//! - a hex, codepoint, or character literal: its decimal value
//!
//! Field text comes from the M2 schema table, function text from
//! `mg_hir::types`, and literal conversions from the same
//! `const_eval::literal_num_value` the compiler uses.

use std::ops::Range;

use mg_hir::schema;
use mg_hir::types::{self, Type};
use mg_syntax::ast::{self, AstNode};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::completion::{field_documentation, function_signatures};
use crate::index::{Def, Index};
use crate::types::NameTypes;

pub struct Ctx<'a> {
    pub root: &'a SyntaxNode,
    pub text: &'a str,
    pub index: &'a Index,
    pub types: &'a NameTypes,
    pub offset: usize,
}

fn code(text: &str) -> String {
    format!("```metaglyph\n{text}\n```")
}

/// The hover text for the token at `ctx.offset`, as markdown, and the
/// span it describes.
pub fn hover(ctx: &Ctx) -> Option<(String, Range<usize>)> {
    let offset = ctx.offset.min(u32::MAX as usize) as u32;
    // At a boundary (`capHeight.|y`) two tokens touch the cursor; the
    // name or number is the one to describe, never the punctuation.
    let token = ctx.root.token_at_offset(offset.into()).find(|t| {
        matches!(
            t.kind(),
            SyntaxKind::IDENT
                | SyntaxKind::NUMBER_ANGLE
                | SyntaxKind::NUMBER_RATIO
                | SyntaxKind::NUMBER_HEX
                | SyntaxKind::NUMBER_CODEPOINT
                | SyntaxKind::NUMBER_CHAR
        )
    })?;
    let range: Range<usize> = token.text_range().into();
    let text = match token.kind() {
        SyntaxKind::NUMBER_ANGLE | SyntaxKind::NUMBER_RATIO => suffixed(ctx, &token)?,
        SyntaxKind::NUMBER_HEX | SyntaxKind::NUMBER_CODEPOINT | SyntaxKind::NUMBER_CHAR => {
            integer(&token)?
        }
        SyntaxKind::IDENT => ident(ctx, &token)?,
        _ => return None,
    };
    Some((text, range))
}

fn suffixed(ctx: &Ctx, token: &SyntaxToken) -> Option<String> {
    let literal = token.text();
    let value = mg_hir::const_eval::literal_num_value(token, ctx.index.em);
    Some(match (token.kind(), value) {
        (SyntaxKind::NUMBER_ANGLE, Some(v)) => format!("`{literal}` = {} rad", round4(v)),
        (_, Some(v)) if literal.ends_with("em") => format!(
            "`{literal}` = {} units, at `font.em` = {}",
            round4(v),
            ctx.index.em.unwrap_or_default()
        ),
        (_, Some(v)) => format!("`{literal}` = {}", round4(v)),
        (_, None) => format!("`{literal}` needs a numeric `font.em` to convert"),
    })
}

fn integer(token: &SyntaxToken) -> Option<String> {
    let value = mg_hir::const_eval::literal_num_value(token, None)? as u64;
    Some(if token.kind() == SyntaxKind::NUMBER_HEX {
        format!("`{}` = {value}", token.text())
    } else {
        format!("`{}` = {value} (U+{value:04X})", token.text())
    })
}

/// Four decimal places, without trailing zeros.
fn round4(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn ident(ctx: &Ctx, token: &SyntaxToken) -> Option<String> {
    let parent = token.parent()?;
    let glyph = ctx.index.glyph_at(ctx.offset);
    let glyph_name = glyph.map(|g| ctx.index.glyphs[g].decl.name.as_str());

    // A call's callee: a built-in function.
    if parent.kind() == SyntaxKind::IDENT_EXPR
        && let Some(call) = parent.parent().and_then(ast::CallExpr::cast)
        && call.callee().is_some_and(|c| c.syntax() == &parent)
    {
        let doc = types::function_doc(token.text())?;
        return Some(format!(
            "{}\n\n{}",
            code(&function_signatures(token.text())?),
            doc.doc
        ));
    }

    // A field name: its schema entry, or the param an instance overrides.
    if let Some(field) = ast::Field::cast(parent.clone())
        && field.name_token().as_ref() == Some(token)
    {
        let block = parent.parent()?.parent()?.kind();
        if let Some(entry) = schema::fields_for(block)
            .and_then(|table| table.iter().find(|f| f.name == token.text()))
        {
            return Some(format!(
                "{}\n\n{}",
                code(&format!("{}: {}", entry.name, entry.ty)),
                field_documentation(entry)
            ));
        }
    }

    // A member of a namespace or of a typed value.
    if let Some(member) = ast::MemberExpr::cast(parent.clone())
        && member.member_token().as_ref() == Some(token)
        && ctx.index.resolve(token).is_none()
    {
        let receiver = member.receiver()?;
        let ty = ctx.types.member(glyph_name, &receiver, token.text())?;
        let root = receiver.syntax().text().to_string();
        let root = root.trim();
        return Some(code(&format!("{root}.{}: {ty}", token.text())));
    }

    if let Some(def) = ctx.index.resolve(token) {
        return name(ctx, &def);
    }

    types::builtin_constant(token.text()).map(|ty| {
        format!(
            "{}\n\nBuilt-in constant.",
            code(&format!("{}: {ty}", token.text()))
        )
    })
}

/// `name: type`, prefixed by its declaration kind, and its declaration
/// line.
fn name(ctx: &Ctx, def: &Def) -> Option<String> {
    let index = ctx.index;
    let has = |decls: &[crate::index::Decl], name: &str| decls.iter().any(|d| d.name == name);
    let typed = |kind: &str, name: &str, ty: Option<Type>| match ty {
        Some(ty) => format!("{kind} {name}: {ty}"),
        None => format!("{kind} {name}"),
    };
    let heading = match def {
        Def::TopLevel(name) => {
            let kind = if has(&index.params, name) {
                "param"
            } else if has(&index.metrics, name) {
                "metric"
            } else {
                "let"
            };
            typed(kind, name, ctx.types.name(None, name))
        }
        Def::Glyph(name) => format!("glyph {name}"),
        Def::Group(name) => format!("group {name}"),
        Def::GlyphSet(name) => format!("glyph set {name}"),
        Def::GlyphLocal { glyph, name } => {
            let entry = &index.glyphs[*glyph];
            let kind = if has(&entry.lets, name) {
                "let"
            } else if has(&entry.anchors, name) {
                "anchor"
            } else {
                "path"
            };
            let ty = ctx
                .types
                .glyphs
                .get(&entry.decl.name)
                .and_then(|scope| scope.get(name))
                .cloned()
                .filter(|t| *t != Type::Error);
            format!("{} (in glyph {})", typed(kind, name, ty), entry.decl.name)
        }
        Def::Segment { glyph, path, name } => {
            let path = index.glyphs[*glyph].paths[*path]
                .decl
                .as_ref()
                .map_or("an anonymous path".to_string(), |d| {
                    format!("path {}", d.name)
                });
            format!("segment {name} (in {path})")
        }
    };

    let declared = index.definition(def)?;
    let line_start = ctx.text[..declared.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = ctx.text[declared.start..]
        .find('\n')
        .map_or(ctx.text.len(), |i| declared.start + i);
    let line_number = ctx.text[..declared.start].matches('\n').count() + 1;
    let line = ctx.text[line_start..line_end].trim();

    Some(format!(
        "{}\n\nDeclared on line {line_number}:\n{}",
        code(&heading),
        code(line)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every schema entry has hover text (plan 4, L2).
    #[test]
    fn every_schema_field_has_hover_text() {
        let kinds = [
            SyntaxKind::FONT,
            SyntaxKind::PARAM,
            SyntaxKind::METRIC,
            SyntaxKind::GLYPH,
            SyntaxKind::INSTANCE,
            SyntaxKind::GROUP,
            SyntaxKind::KERN,
            SyntaxKind::PATH,
            SyntaxKind::ANCHOR,
            SyntaxKind::COMPONENT,
            SyntaxKind::START,
            SyntaxKind::LINE,
            SyntaxKind::QUAD,
            SyntaxKind::CUBE,
            SyntaxKind::ARC,
            SyntaxKind::CLOSE,
        ];
        for kind in kinds {
            for field in schema::fields_for(kind).unwrap() {
                assert!(!field.doc.is_empty(), "{kind:?}.{}", field.name);
                assert!(!field.ty.is_empty(), "{kind:?}.{}", field.name);
                // A map's values are per entry; an entry left out falls
                // back elsewhere (`joinAt` to `joins`), so it has no
                // default of its own.
                let is_map = field.ty.starts_with("map<");
                assert!(
                    field.required || field.default.is_some() || field.values.is_empty() || is_map,
                    "{kind:?}.{}: an optional enum needs a default",
                    field.name
                );
            }
        }
    }

    #[test]
    fn numbers_round_to_four_places_without_trailing_zeros() {
        assert_eq!(round4(std::f64::consts::PI), "3.1416");
        assert_eq!(round4(0.25), "0.25");
        assert_eq!(round4(50.0), "50");
    }
}
