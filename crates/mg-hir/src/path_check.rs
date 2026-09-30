//! Path structural checks (spec §5.7, §6.3): every rule that depends on a
//! segment's position among its siblings, which is exactly what
//! `crate::schema`'s per-field table cannot see. Run once per path, after
//! the path is lowered.

use mg_diag::{Diagnostic, Label};

use crate::model::{PathDecl, SegmentKind};
use mg_diag::codes;

pub fn check_path(path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    let has_body = !path.segments.is_empty() || path_has_close_only(path);

    if has_body {
        check_body_shape(path, diagnostics);
    } else {
        diagnostics.push(Diagnostic::error(
            codes::PATH_NEEDS_BODY,
            "a path must have a body",
            Label::new(crate::schema::trimmed_span(&path.syntax), "no body"),
        ));
    }

    if path.fill && !path.closed {
        diagnostics.push(Diagnostic::error(
            codes::FILL_REQUIRES_CLOSED_PATH,
            "`fill` requires a closed path",
            Label::new(
                crate::schema::trimmed_span(&path.syntax),
                "path is not closed",
            ),
        ));
    }

    if path.caps.is_some() && path.closed {
        diagnostics.push(Diagnostic::error(
            codes::FIELD_ILLEGAL_HERE,
            "`caps` requires an open path",
            Label::new(crate::schema::trimmed_span(&path.syntax), "path is closed"),
        ));
    }
}

/// A path whose body is only `close` (no `start`) still "has a body" for
/// the purposes of the body check; `check_body_shape` then
/// reports the missing `start` itself.
fn path_has_close_only(path: &PathDecl) -> bool {
    path.segments.is_empty() && path.closed
}

fn check_body_shape(path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    match path.segments.first() {
        Some(first) if first.kind == SegmentKind::Start => {}
        _ => diagnostics.push(Diagnostic::error(
            codes::PATH_MISSING_START,
            "a path body must begin with `start`",
            Label::new(
                crate::schema::trimmed_span(&path.syntax),
                "no `start` declaration",
            ),
        )),
    }

    for (i, seg) in path.segments.iter().enumerate() {
        if seg.kind != SegmentKind::Start {
            continue;
        }
        if i != 0 {
            let code = if path.segments[..i]
                .iter()
                .any(|s| s.kind == SegmentKind::Start)
            {
                codes::MULTIPLE_START
            } else {
                codes::START_NOT_FIRST
            };
            diagnostics.push(Diagnostic::error(
                code,
                "`start` must be the first declaration in a path body",
                Label::new(
                    crate::schema::trimmed_span(&seg.syntax),
                    "not the first declaration",
                ),
            ));
        }
    }

    let close_count = usize::from(path.closed);
    if close_count > 1 {
        // Unreachable today (the CST only ever holds `path.closed` as a
        // bool set from at most one `close` node), kept as a guard in case
        // `crate::lower` ever starts tracking `close` nodes individually.
        diagnostics.push(Diagnostic::error(
            codes::MULTIPLE_CLOSE,
            "a path body may declare `close` at most once",
            Label::new(
                crate::schema::trimmed_span(&path.syntax),
                "multiple `close`",
            ),
        ));
    }

    check_reflections(path, diagnostics);
}

/// A `quad` without `c`, or a `cube` without `c1`, reflects the previous
/// declaration's adjacent control point (spec §6.3) — legal only when
/// that previous declaration is a segment of the same kind.
fn check_reflections(path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    for (i, seg) in path.segments.iter().enumerate() {
        let (omitted, field_name) = match seg.kind {
            SegmentKind::Quad => (seg.c.is_none(), "c"),
            SegmentKind::Cube => (seg.c1.is_none(), "c1"),
            _ => continue,
        };
        if !omitted {
            continue;
        }
        let previous_matches = i > 0 && path.segments[i - 1].kind == seg.kind;
        if !previous_matches {
            diagnostics.push(Diagnostic::error(
                codes::INVALID_REFLECTION,
                format!(
                    "`{field_name}` may only be omitted when the previous declaration is also a `{}`",
                    segment_keyword(seg.kind)
                ),
                Label::new(
                    crate::schema::trimmed_span(&seg.syntax),
                    format!("missing `{field_name}`"),
                ),
            ));
        }
    }
}

fn segment_keyword(kind: SegmentKind) -> &'static str {
    match kind {
        SegmentKind::Start => "start",
        SegmentKind::Line => "line",
        SegmentKind::Quad => "quad",
        SegmentKind::Cube => "cube",
        SegmentKind::Arc => "arc",
    }
}
