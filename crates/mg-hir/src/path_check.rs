//! Path structural checks (spec §5.7, §6.3): every rule that depends on a
//! segment's position among its siblings, which is exactly what
//! `crate::schema`'s per-field table cannot see. Run once per path, after
//! the whole glyph (and so every `follows` target) is lowered.

use mg_diag::{Diagnostic, Label};

use crate::model::{PathDecl, SegmentDecl, SegmentKind};
use mg_diag::codes;

pub fn check_path(all_paths: &[PathDecl], path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    let has_body = !path.segments.is_empty() || path_has_close_only(path);

    match (&path.follows, has_body) {
        (Some(_), true) => diagnostics.push(Diagnostic::error(
            codes::MUTUALLY_EXCLUSIVE_FIELDS,
            "`follows` is mutually exclusive with a body",
            Label::new(
                crate::schema::trimmed_span(&path.syntax),
                "has both `follows` and a body",
            ),
        )),
        (None, false) => diagnostics.push(Diagnostic::error(
            codes::PATH_NEEDS_BODY_OR_FOLLOWS,
            "a path must have either a body or `follows`",
            Label::new(crate::schema::trimmed_span(&path.syntax), "has neither"),
        )),
        _ => {}
    }

    if let Some(target_name) = &path.follows {
        match all_paths
            .iter()
            .find(|p| p.name.as_deref() == Some(target_name.as_str()))
        {
            Some(target) if target.follows.is_none() && !target.segments.is_empty() => {}
            Some(_) => diagnostics.push(Diagnostic::error(
                codes::FOLLOWS_TARGET_HAS_NO_BODY,
                format!("`{target_name}` has no body of its own to follow"),
                Label::new(
                    crate::schema::trimmed_span(&path.syntax),
                    "follows a bodyless path",
                ),
            )),
            // An unresolved `follows` target is reported at the field
            // itself, by `crate::lower`; nothing more to add here.
            None => {}
        }
    }

    if has_body {
        check_body_shape(path, diagnostics);
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
/// the purposes of the body-vs-`follows` check; `check_body_shape` then
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

    check_start_fields(path, diagnostics);
    check_spline_fields(path, diagnostics);
}

fn check_start_fields(path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    let Some(start) = path
        .segments
        .first()
        .filter(|s| s.kind == SegmentKind::Start)
    else {
        return;
    };
    let next = path.segments.get(1);

    if start.dir.is_some() && !path.closed && next.is_some_and(|seg| seg.kind == SegmentKind::Line)
    {
        diagnostics.push(Diagnostic::error(
            codes::FIELD_ILLEGAL_HERE,
            "`dir` on `start` is illegal when the first segment is a `line` on an open path",
            Label::new(crate::schema::trimmed_span(&start.syntax), "illegal here"),
        ));
    }

    if start.curl.is_some()
        && !(!path.closed && next.is_some_and(|seg| seg.kind == SegmentKind::Spline))
    {
        diagnostics.push(Diagnostic::error(
            codes::FIELD_ILLEGAL_HERE,
            "`curl` on `start` is legal only on an open path whose first segment is a `spline`",
            Label::new(crate::schema::trimmed_span(&start.syntax), "illegal here"),
        ));
    }
}

fn check_spline_fields(path: &PathDecl, diagnostics: &mut Vec<Diagnostic>) {
    let last_index = path.segments.len().saturating_sub(1);
    for (i, seg) in path.segments.iter().enumerate() {
        if seg.kind != SegmentKind::Spline {
            continue;
        }
        check_from_dir_position(path, i, seg, diagnostics);
        check_curl_position(path, i, last_index, seg, diagnostics);
    }
}

fn check_from_dir_position(
    path: &PathDecl,
    index: usize,
    seg: &SegmentDecl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let first_after_start = path
        .segments
        .first()
        .is_some_and(|s| s.kind == SegmentKind::Start)
        && index == 1;
    if seg.from_dir.is_some() && first_after_start {
        diagnostics.push(Diagnostic::error(
            codes::FIELD_ILLEGAL_HERE,
            "`fromDir` is illegal on the first segment after `start`",
            Label::new(crate::schema::trimmed_span(&seg.syntax), "illegal here"),
        ));
    }
}

fn check_curl_position(
    path: &PathDecl,
    index: usize,
    last_index: usize,
    seg: &SegmentDecl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if seg.curl.is_some() && !(index == last_index && !path.closed) {
        diagnostics.push(Diagnostic::error(
            codes::FIELD_ILLEGAL_HERE,
            "`curl` is legal only on the final segment of an open path",
            Label::new(crate::schema::trimmed_span(&seg.syntax), "illegal here"),
        ));
    }
}
