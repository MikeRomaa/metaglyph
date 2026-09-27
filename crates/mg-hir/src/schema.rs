//! Declarative field schemas (spec §5.6–§5.7): the field-*existence*
//! shape of every block kind — legal names, required/optional, mutual
//! exclusions, and "requires another field" dependencies — factored into
//! one table per kind so "unknown field," "missing required field," and
//! "mutually exclusive fields" fire the same way everywhere (spec plan
//! M2: "That single table produces every field-validation error... Do not
//! scatter these checks"). A field's *value* (type, constant-ness, enum
//! legality) and positional rules that need sibling context (`caps`
//! needing an open path, `curl` needing to be the final segment) are still
//! checked where the field is used, in `crate::lower` and
//! `crate::path_check` respectively — this table has no such context.

use std::ops::Range;

use indexmap::IndexMap;
use mg_diag::{Diagnostic, Label};
use mg_syntax::SyntaxNode;
use mg_syntax::ast::{self, AstNode};

use crate::codes;

/// `node`'s span, minus any leading trivia nested inside it. A block node
/// (spec §5.2: `<kind> <name>? (config)? (body)?`) is opened, in the
/// parser, before the whitespace between it and the previous declaration
/// is flushed — the same reason [`mg_syntax::ast::Literal::token`] exists
/// — so anchoring a diagnostic directly on `node.text_range()` would
/// underline that leading gap instead of the declaration itself.
pub fn trimmed_span(node: &SyntaxNode) -> Range<usize> {
    let full: Range<usize> = node.text_range().into();
    let start = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| Range::<usize>::from(t.text_range()).start)
        .unwrap_or(full.start);
    start..full.end
}

pub struct FieldSchema {
    pub name: &'static str,
    pub required: bool,
    /// Other field names this one may not co-occur with.
    pub mutex: &'static [&'static str],
    /// Other field names that must also be present for this one to be
    /// legal (spec §5.7: `caps`, `joins`, and `joinAt` all "require
    /// `stroke`").
    pub requires: &'static [&'static str],
}

const fn field(name: &'static str, required: bool) -> FieldSchema {
    FieldSchema {
        name,
        required,
        mutex: &[],
        requires: &[],
    }
}

const fn with_mutex(f: FieldSchema, mutex: &'static [&'static str]) -> FieldSchema {
    FieldSchema { mutex, ..f }
}

const fn with_requires(f: FieldSchema, requires: &'static [&'static str]) -> FieldSchema {
    FieldSchema { requires, ..f }
}

pub const FONT_FIELDS: &[FieldSchema] = &[
    field("name", true),
    field("em", true),
    field("version", false),
    field("designer", false),
    field("foundry", false),
    field("license", false),
];

pub const PARAM_FIELDS: &[FieldSchema] = &[field("default", true), field("range", false)];

pub const METRIC_FIELDS: &[FieldSchema] = &[
    field("y", true),
    field("overshoot", false),
    field("align", false),
];

pub const GLYPH_FIELDS: &[FieldSchema] = &[
    with_mutex(field("codepoint", false), &["glyphset"]),
    field("advance", true),
    with_mutex(field("glyphset", false), &["codepoint"]),
];

/// The instance fields fixed by spec §5.6; any declared `param` name is
/// also legal and is validated separately in `crate::lower`, since this
/// table cannot see the font's param list.
pub const INSTANCE_FIXED_FIELDS: &[FieldSchema] = &[
    field("slant", false),
    field("glyphset", false),
    field("styleName", false),
    field("weightClass", false),
    field("widthClass", false),
];

pub const GROUP_FIELDS: &[FieldSchema] = &[field("glyphs", true)];

pub const KERN_FIELDS: &[FieldSchema] =
    &[field("left", true), field("right", true), field("by", true)];

pub const PATH_FIELDS: &[FieldSchema] = &[
    field("follows", false),
    field("stroke", false),
    field("fill", false),
    with_requires(field("caps", false), &["stroke"]),
    with_requires(field("joins", false), &["stroke"]),
    with_requires(field("joinAt", false), &["stroke"]),
    field("enabled", false),
];

pub const START_FIELDS: &[FieldSchema] =
    &[field("at", true), field("dir", false), field("curl", false)];

pub const LINE_FIELDS: &[FieldSchema] = &[field("to", true)];

pub const SPLINE_FIELDS: &[FieldSchema] = &[
    field("to", true),
    with_mutex(field("dir", false), &["controls"]),
    with_mutex(field("fromDir", false), &["controls"]),
    with_mutex(field("tension", false), &["controls"]),
    with_mutex(field("controls", false), &["dir", "fromDir", "tension"]),
    field("curl", false),
];

pub const ANCHOR_FIELDS: &[FieldSchema] = &[field("at", true)];

pub const COMPONENT_FIELDS: &[FieldSchema] = &[
    field("glyph", true),
    with_mutex(field("offset", false), &["transform"]),
    with_mutex(field("transform", false), &["offset"]),
];

pub const CLOSE_FIELDS: &[FieldSchema] = &[];

/// Validates a config's field *shape* against `schema` (unknown, missing,
/// mutually exclusive, and "requires" checks), returning every legal field
/// actually present, keyed by name, for the caller to pull values from.
/// `block_syntax` anchors "missing required field," which has no field
/// token of its own to point at.
pub fn collect_fields(
    block_syntax: &SyntaxNode,
    config: Option<&ast::Config>,
    schema: &[FieldSchema],
    block_desc: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> IndexMap<String, ast::Field> {
    let legal: Vec<&str> = schema.iter().map(|f| f.name).collect();
    let mut seen: IndexMap<String, ast::Field> = IndexMap::new();

    if let Some(config) = config {
        for field in config.fields() {
            let Some(token) = field.name_token() else {
                continue;
            };
            let name = token.text().to_string();
            let span: Range<usize> = token.text_range().into();

            if !legal.contains(&name.as_str()) {
                let diagnostic = Diagnostic::error(
                    codes::UNKNOWN_FIELD,
                    format!("{block_desc} has no field `{name}`"),
                    Label::new(span.clone(), "unknown field"),
                );
                let diagnostic = match mg_diag::suggest::nearest_match(&name, legal.iter().copied())
                {
                    Some(suggestion) => {
                        diagnostic.with_help(format!("did you mean `{suggestion}`?"))
                    }
                    None => diagnostic,
                };
                diagnostics.push(diagnostic);
                continue;
            }

            if seen.contains_key(&name) {
                diagnostics.push(Diagnostic::error(
                    codes::DUPLICATE_DEFINITION,
                    format!("field `{name}` given more than once"),
                    Label::new(span, "duplicate field"),
                ));
                continue;
            }

            seen.insert(name, field);
        }
    }

    let anchor_span = trimmed_span(block_syntax);

    for spec in schema {
        if spec.required && !seen.contains_key(spec.name) {
            diagnostics.push(Diagnostic::error(
                codes::MISSING_REQUIRED_FIELD,
                format!("{block_desc} is missing required field `{}`", spec.name),
                Label::new(anchor_span.clone(), format!("missing `{}`", spec.name)),
            ));
        }
    }

    for spec in schema {
        let Some(this_field) = seen.get(spec.name) else {
            continue;
        };
        for other in spec.mutex {
            // Mutex is declared symmetrically in every table above; only
            // report once per pair, anchored on the alphabetically-first
            // name, so a symmetric declaration cannot double-report.
            if seen.contains_key(*other) && spec.name < *other {
                let this_span: Range<usize> = this_field.syntax().text_range().into();
                diagnostics.push(Diagnostic::error(
                    codes::MUTUALLY_EXCLUSIVE_FIELDS,
                    format!("`{}` and `{other}` are mutually exclusive", spec.name),
                    Label::new(this_span, format!("also given `{other}` below")),
                ));
            }
        }
        for required in spec.requires {
            if !seen.contains_key(*required) {
                let this_span: Range<usize> = this_field.syntax().text_range().into();
                diagnostics.push(Diagnostic::error(
                    codes::FIELD_ILLEGAL_HERE,
                    format!("`{}` requires `{required}`", spec.name),
                    Label::new(this_span, format!("needs `{required}`")),
                ));
            }
        }
    }

    seen
}
