//! Declarative field schemas (spec §5.6–§5.7): every block kind's fields —
//! legal names, required/optional, mutual exclusions, "requires another
//! field" dependencies, and each field's type, default, enum set, and
//! description — in one table per kind. Lowering validates against these
//! tables and the language server's completion and hover read them, so
//! neither keeps its own copy (spec plan M2: "That single table produces
//! every field-validation error... Do not scatter these checks").
//!
//! [`collect_fields`] enforces existence: unknown, missing, mutually
//! exclusive, and "requires" fields. A field's *value* (type,
//! constant-ness, membership in `values`) and positional rules that need
//! sibling context (`caps` needing an open path, an omitted `c`/`c1`
//! needing the previous declaration to be the same kind) are checked where
//! the field is used, in `crate::lower` and `crate::path_check` — this
//! table has no such context.

use std::ops::Range;

use indexmap::IndexMap;
use mg_diag::{Diagnostic, Label};
use mg_syntax::ast::{self, AstNode};
use mg_syntax::{SyntaxKind, SyntaxNode};

use mg_diag::codes;

pub use mg_syntax::trimmed_range as trimmed_span;

pub struct FieldSchema {
    pub name: &'static str,
    pub required: bool,
    /// Other field names this one may not co-occur with.
    pub mutex: &'static [&'static str],
    /// Other field names that must also be present for this one to be
    /// legal (spec §5.7: `caps`, `joins`, and `joinAt` all "require
    /// `stroke`").
    pub requires: &'static [&'static str],
    /// The value's type as the spec writes it (§5.5–§5.7): `num`,
    /// `point`, `glyphref`, `map<segmentName, string>`, ….
    pub ty: &'static str,
    /// The value used when the field is omitted, as source text.
    pub default: Option<&'static str>,
    /// The legal strings, for an enum-valued field (spec §5.5). Lowering
    /// validates against exactly this list.
    pub values: &'static [&'static str],
    /// One line for hover and completion (spec §5.6–§5.7).
    pub doc: &'static str,
}

const fn field(
    name: &'static str,
    required: bool,
    ty: &'static str,
    doc: &'static str,
) -> FieldSchema {
    FieldSchema {
        name,
        required,
        mutex: &[],
        requires: &[],
        ty,
        default: None,
        values: &[],
        doc,
    }
}

const fn with_mutex(f: FieldSchema, mutex: &'static [&'static str]) -> FieldSchema {
    FieldSchema { mutex, ..f }
}

const fn with_requires(f: FieldSchema, requires: &'static [&'static str]) -> FieldSchema {
    FieldSchema { requires, ..f }
}

const fn with_default(f: FieldSchema, default: &'static str) -> FieldSchema {
    FieldSchema {
        default: Some(default),
        ..f
    }
}

const fn with_values(f: FieldSchema, values: &'static [&'static str]) -> FieldSchema {
    FieldSchema { values, ..f }
}

/// `joins` and each `joinAt` value (spec §5.5).
pub const JOIN_VALUES: &[&str] = &["miter", "round", "bevel"];
/// Each end of `caps` (spec §5.5).
pub const CAP_VALUES: &[&str] = &["butt", "round", "square"];

pub const FONT_FIELDS: &[FieldSchema] = &[
    field("name", true, "string", "The family name."),
    field(
        "em",
        true,
        "int",
        "Units per em; an integer literal between 16 and 16384.",
    ),
    with_default(
        field(
            "version",
            false,
            "string",
            "Of the form `digits.digits`; feeds `head.fontRevision` and name ID 5.",
        ),
        "\"1.000\"",
    ),
    field("designer", false, "string", "Name ID 9."),
    field("foundry", false, "string", "Name ID 8."),
    field("license", false, "string", "Name ID 13."),
];

pub const PARAM_FIELDS: &[FieldSchema] = &[
    field(
        "default",
        true,
        "num",
        "The value when no instance overrides it; a constant expression.",
    ),
    field(
        "range",
        false,
        "range",
        "`lo..hi`, inclusive; the default and every override must lie within it.",
    ),
];

pub const METRIC_FIELDS: &[FieldSchema] = &[
    field(
        "y",
        true,
        "num",
        "The flat position; may reference params and top-level `let`s.",
    ),
    with_default(
        field(
            "overshoot",
            false,
            "num",
            "How far round glyphs pass `y`; must be ≥ 0.",
        ),
        "0",
    ),
    with_values(
        with_default(
            field(
                "align",
                false,
                "string",
                "Which way the overshoot goes: `.ink` is `y + overshoot` for \"top\", `y − overshoot` for \"bottom\".",
            ),
            "\"top\"",
        ),
        &["top", "bottom"],
    ),
];

pub const GLYPH_FIELDS: &[FieldSchema] = &[
    with_mutex(
        field(
            "codepoint",
            false,
            "int | int*",
            "Each value in 0–0x10FFFF; a constant expression. Illegal with `glyphset`.",
        ),
        &["glyphset"],
    ),
    field("advance", true, "num", "The advance width."),
    with_mutex(
        field(
            "glyphset",
            false,
            "identifier",
            "Makes this the alternate, in that set, of the default glyph with the same name.",
        ),
        &["codepoint"],
    ),
];

/// The instance fields fixed by spec §5.6; any declared `param` name is
/// also legal and is validated separately in `crate::lower`, since this
/// table cannot see the font's param list.
pub const INSTANCE_FIXED_FIELDS: &[FieldSchema] = &[
    with_default(
        field(
            "slant",
            false,
            "num",
            "An angle; shears every outline, positive leaning right. A constant expression.",
        ),
        "0",
    ),
    field(
        "glyphset",
        false,
        "identifier",
        "Builds each glyph's alternate in this set where one exists.",
    ),
    with_default(
        field("styleName", false, "string", "The style name."),
        "the instance's name",
    ),
    with_default(
        field("weightClass", false, "int", "`OS/2.usWeightClass`; 1–1000."),
        "400",
    ),
    with_default(
        field("widthClass", false, "int", "`OS/2.usWidthClass`; 1–9."),
        "5",
    ),
];

pub const GROUP_FIELDS: &[FieldSchema] = &[field(
    "glyphs",
    true,
    "glyphref*",
    "A non-empty list of default-set glyphs.",
)];

pub const KERN_FIELDS: &[FieldSchema] = &[
    field(
        "left",
        true,
        "glyphref | groupref",
        "The first glyph of the pair, or a group of them.",
    ),
    field(
        "right",
        true,
        "glyphref | groupref",
        "The second glyph of the pair, or a group of them.",
    ),
    field(
        "by",
        true,
        "num",
        "The adjustment to the left glyph's advance; rounded.",
    ),
];

pub const PATH_FIELDS: &[FieldSchema] = &[
    field(
        "follows",
        false,
        "pathref",
        "Takes another path's skeleton exactly; mutually exclusive with a body.",
    ),
    field(
        "stroke",
        false,
        "num",
        "The stroke width, constant along the path; must be > 0.",
    ),
    with_default(
        field(
            "fill",
            false,
            "bool",
            "Inks the interior; requires a closed path. Combines with `stroke`.",
        ),
        "false",
    ),
    with_values(
        with_default(
            with_requires(
                field(
                    "caps",
                    false,
                    "string | (string, string)",
                    "One cap for both ends, or `(start, end)`. Requires `stroke` and an open path.",
                ),
                &["stroke"],
            ),
            "\"butt\"",
        ),
        CAP_VALUES,
    ),
    with_values(
        with_default(
            with_requires(
                field(
                    "joins",
                    false,
                    "string",
                    "The join at every corner. Requires `stroke`.",
                ),
                &["stroke"],
            ),
            "\"miter\"",
        ),
        JOIN_VALUES,
    ),
    with_values(
        with_requires(
            field(
                "joinAt",
                false,
                "map<segmentName, string>",
                "Joins for single corners, keyed by the segment ending there. Requires `stroke`.",
            ),
            &["stroke"],
        ),
        JOIN_VALUES,
    ),
    with_default(
        field(
            "enabled",
            false,
            "bool",
            "`false` keeps the path for construction without rendering it.",
        ),
        "true",
    ),
];

pub const START_FIELDS: &[FieldSchema] = &[field("at", true, "point", "Sets the current point.")];

pub const LINE_FIELDS: &[FieldSchema] = &[field("to", true, "point", "The endpoint.")];

pub const QUAD_FIELDS: &[FieldSchema] = &[
    field("to", true, "point", "The endpoint."),
    field(
        "c",
        false,
        "point",
        "The control point. Omit only after a `quad`, to reflect its control point.",
    ),
];

pub const CUBE_FIELDS: &[FieldSchema] = &[
    field("to", true, "point", "The endpoint."),
    field(
        "c1",
        false,
        "point",
        "The first control point. Omit only after a `cube`, to reflect its `c2`.",
    ),
    field("c2", true, "point", "The second control point."),
];

/// `center` and `rx`/`ry` are two mutually exclusive ways to fix an
/// `arc`'s ellipse (spec §6.3 centre mode / radii mode); at least one must
/// be given, which `crate::lower::check_arc_mode` checks separately since
/// this table only expresses per-field shape, not "one of these groups."
pub const ARC_FIELDS: &[FieldSchema] = &[
    field("to", true, "point", "The endpoint."),
    with_values(
        field(
            "sweep",
            true,
            "string",
            "The direction of travel about the centre, y-up.",
        ),
        &["ccw", "cw"],
    ),
    with_mutex(
        field(
            "center",
            false,
            "point",
            "Centre mode: the ellipse's centre; the radii are solved.",
        ),
        &["rx", "ry"],
    ),
    with_requires(
        with_mutex(
            field(
                "rx",
                false,
                "num",
                "Radii mode: the horizontal radius, > 0; the centre is solved.",
            ),
            &["center"],
        ),
        &["ry"],
    ),
    with_requires(
        with_mutex(
            field(
                "ry",
                false,
                "num",
                "Radii mode: the vertical radius, > 0; the centre is solved.",
            ),
            &["center"],
        ),
        &["rx"],
    ),
    with_default(
        with_requires(
            field(
                "large",
                false,
                "bool",
                "Radii mode: take the arc spanning more than 180°.",
            ),
            &["rx"],
        ),
        "false",
    ),
];

pub const ANCHOR_FIELDS: &[FieldSchema] = &[field(
    "at",
    true,
    "point",
    "The anchor's position; read as `glyphs.<name>.<anchor>`.",
)];

pub const COMPONENT_FIELDS: &[FieldSchema] = &[
    field(
        "glyph",
        true,
        "glyphref",
        "The default-set glyph to place; follows the instance's glyph set.",
    ),
    with_mutex(
        field("offset", false, "pair", "Places it by `translate(dx, dy)`."),
        &["transform"],
    ),
    with_mutex(
        field(
            "transform",
            false,
            "transform",
            "Places it by any transform.",
        ),
        &["offset"],
    ),
];

pub const CLOSE_FIELDS: &[FieldSchema] = &[];

/// The fixed field table for a declaration of `kind`, or `None` for a
/// kind that takes no config at all (`let`). An instance also accepts
/// every declared param, which no fixed table can list.
pub fn fields_for(kind: SyntaxKind) -> Option<&'static [FieldSchema]> {
    Some(match kind {
        SyntaxKind::FONT => FONT_FIELDS,
        SyntaxKind::PARAM => PARAM_FIELDS,
        SyntaxKind::METRIC => METRIC_FIELDS,
        SyntaxKind::GLYPH => GLYPH_FIELDS,
        SyntaxKind::INSTANCE => INSTANCE_FIXED_FIELDS,
        SyntaxKind::GROUP => GROUP_FIELDS,
        SyntaxKind::KERN => KERN_FIELDS,
        SyntaxKind::PATH => PATH_FIELDS,
        SyntaxKind::ANCHOR => ANCHOR_FIELDS,
        SyntaxKind::COMPONENT => COMPONENT_FIELDS,
        SyntaxKind::START => START_FIELDS,
        SyntaxKind::LINE => LINE_FIELDS,
        SyntaxKind::QUAD => QUAD_FIELDS,
        SyntaxKind::CUBE => CUBE_FIELDS,
        SyntaxKind::ARC => ARC_FIELDS,
        SyntaxKind::CLOSE => CLOSE_FIELDS,
        _ => return None,
    })
}

/// `name`'s entry in `schema`. Panics on a name not in the table, which
/// is a bug in the caller, not in the source being checked.
pub fn entry(schema: &'static [FieldSchema], name: &str) -> &'static FieldSchema {
    schema
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field `{name}` in this schema"))
}

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
