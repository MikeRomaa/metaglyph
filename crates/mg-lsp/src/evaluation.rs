//! Evaluation-backed diagnostics and hover values (plan 4, L3).
//!
//! Every instance is evaluated in full, off the keystroke path: the
//! server waits until edits pause (see `crate::server`), and the
//! evaluator checks a cancel flag between graph nodes so a new message
//! abandons a stale run. The Appendix A sample evaluates all three
//! instances in about 4 ms (release build), so re-evaluating everything
//! beats tracking which nodes an edit dirtied; plan 3's dirty-set API is
//! there if a real character set proves otherwise.
//!
//! Export-class diagnostics (spec §13 "Export") need a full build and
//! are not run here.

use indexmap::IndexMap;
use mg_eval::value::Value;
use mg_eval::{EvalOutcome, NodeId};
use mg_hir::Hir;

use crate::index::{Def, Index};

/// One instance's evaluation.
pub struct InstanceResult {
    pub name: String,
    pub glyphset: Option<String>,
    pub outcome: EvalOutcome,
}

/// Evaluates every instance of `hir`, in declaration order. `None` when
/// `cancelled` fires first.
pub fn evaluate(hir: &Hir, cancelled: &dyn Fn() -> bool) -> Option<Vec<InstanceResult>> {
    hir.instances
        .values()
        .map(|instance| {
            let (_, outcome) = mg_eval::evaluate_cancellable(hir, instance, cancelled)?;
            Some(InstanceResult {
                name: instance.name.clone(),
                glyphset: instance.glyphset.clone(),
                outcome,
            })
        })
        .collect()
}

/// Every instance's diagnostics, each distinct one once, with the
/// instances it fired in appended to its message:
/// `curvature radius drops below … [Bold]` (plan 4, L3).
pub fn diagnostics(results: &[InstanceResult]) -> Vec<mg_diag::Diagnostic> {
    type Key = (String, String, std::ops::Range<usize>);
    let mut merged: IndexMap<Key, (mg_diag::Diagnostic, Vec<&str>)> = IndexMap::new();
    for result in results {
        for diagnostic in &result.outcome.diagnostics {
            let key = (
                diagnostic.code.as_str().to_string(),
                diagnostic.message.clone(),
                diagnostic.primary.span.clone(),
            );
            merged
                .entry(key)
                .or_insert_with(|| (diagnostic.clone(), Vec::new()))
                .1
                .push(&result.name);
        }
    }
    merged
        .into_values()
        .map(|(mut diagnostic, instances)| {
            diagnostic.message = format!("{} [{}]", diagnostic.message, instances.join(", "));
            diagnostic
        })
        .collect()
}

/// A value as hover shows it: numbers to four places, pairs as `(x, y)`,
/// rects as their four edges, zones as `.y` / `.ink`.
pub fn format_value(value: &Value) -> String {
    let n = |v: f64| {
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" {
            "0".to_string()
        } else {
            s.to_string()
        }
    };
    match value {
        Value::Num(v) => n(*v),
        Value::Pair(p) => format!("({}, {})", n(p.x), n(p.y)),
        Value::Rect(r) => format!(
            "x0 {} · y0 {} · x1 {} · y1 {}",
            n(r.x0),
            n(r.y0),
            n(r.x1),
            n(r.y1)
        ),
        Value::Zone(z) => format!(".y {} / .ink {}", n(z.y), n(z.ink)),
        Value::Line(l) => format!(
            "line ({}, {}) → ({}, {})",
            n(l.p0.x),
            n(l.p0.y),
            n(l.p1.x),
            n(l.p1.y)
        ),
        Value::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(format_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.to_string(),
    }
}

/// The graph node holding `def`'s value, and a label for it when it is
/// not the name itself (a glyph shows its advance).
fn node_of(index: &Index, def: &Def) -> Option<(NodeId, Option<&'static str>)> {
    match def {
        Def::TopLevel(name) => Some((NodeId::TopLevel(name.clone()), None)),
        Def::Glyph(name) => Some((NodeId::GlyphAdvance(name.clone()), Some("advance"))),
        Def::GlyphLocal { glyph, name } => {
            let entry = &index.glyphs[*glyph];
            let glyph_name = entry.decl.name.clone();
            if entry.lets.iter().any(|d| d.name == *name) {
                Some((NodeId::GlyphLocal(glyph_name, name.clone()), None))
            } else if entry.anchors.iter().any(|d| d.name == *name) {
                Some((NodeId::Anchor(glyph_name, name.clone()), None))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether `instance` builds the glyph declaration `glyph` (an index into
/// `index.glyphs`) under its name: an alternate only in instances selecting
/// its set, and a default-set glyph wherever no alternate replaces it.
fn builds(index: &Index, glyph: usize, instance: &InstanceResult) -> bool {
    let entry = &index.glyphs[glyph];
    match (&entry.glyphset, &instance.glyphset) {
        (Some(set), selected) => selected.as_ref() == Some(set),
        (None, None) => true,
        (None, Some(selected)) => !index
            .glyphs
            .iter()
            .any(|g| g.decl.name == entry.decl.name && g.glyphset.as_ref() == Some(selected)),
    }
}

/// `def`'s value in each instance that builds it, as markdown: numbers
/// on one line (`Regular 86 · Bold 128 · Condensed 86`), anything longer
/// one instance per line. A failed evaluation shows its error.
pub fn hover_values(index: &Index, results: &[InstanceResult], def: &Def) -> Option<String> {
    let (node, label) = node_of(index, def)?;
    let span = index.decl(def)?.range.clone();

    let mut entries: Vec<(&str, String, bool)> = Vec::new();
    for result in results {
        if let Def::GlyphLocal { glyph, .. } = def
            && !builds(index, *glyph, result)
        {
            continue;
        }
        let outcome = &result.outcome;
        match outcome.values.get(&node) {
            Some(value) => {
                let short = matches!(value, Value::Num(_) | Value::Bool(_));
                entries.push((&result.name, format_value(value), short));
            }
            None if outcome.failed.contains(&node) => {
                // The error raised inside this declaration, if any; a
                // declaration can also fail only because an input did.
                let reason = outcome
                    .diagnostics
                    .iter()
                    .find(|d| span.contains(&d.primary.span.start))
                    .map_or("an input failed".to_string(), |d| d.message.clone());
                entries.push((&result.name, format!("failed: {reason}"), false));
            }
            None => {}
        }
    }
    if entries.is_empty() {
        return None;
    }

    let body = if entries.iter().all(|(_, _, short)| *short) {
        entries
            .iter()
            .map(|(name, value, _)| format!("{name} {value}"))
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        entries
            .iter()
            .map(|(name, value, _)| format!("- {name}: {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Some(match label {
        Some(label) => format!("{label}: {body}"),
        None => body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_eval::value::{Rect, Zone};

    #[test]
    fn values_format_compactly() {
        assert_eq!(format_value(&Value::Num(86.0)), "86");
        assert_eq!(format_value(&Value::Num(0.123456)), "0.1235");
        assert_eq!(format_value(&Value::Num(-0.00001)), "0");
        assert_eq!(
            format_value(&Value::Pair(kurbo::Point::new(1.5, -2.0))),
            "(1.5, -2)"
        );
        assert_eq!(
            format_value(&Value::Rect(Rect {
                x0: 0.0,
                y0: -10.0,
                x1: 500.0,
                y1: 712.0
            })),
            "x0 0 · y0 -10 · x1 500 · y1 712"
        );
        assert_eq!(
            format_value(&Value::Zone(Zone {
                y: 700.0,
                overshoot: 12.0,
                ink: 712.0
            })),
            ".y 700 / .ink 712"
        );
        assert_eq!(
            format_value(&Value::List(vec![Value::Num(1.0), Value::Num(2.5)])),
            "[1, 2.5]"
        );
    }
}
