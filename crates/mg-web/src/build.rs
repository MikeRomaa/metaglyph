//! TTF export (plan 6, W8): every instance built exactly as `mg build`
//! builds it, so an export is byte-identical to the CLI's for the same
//! timestamp.

use mg_font::{BuildOptions, BuiltFont};

use crate::doc::{DiagnosticInfo, Model, diagnostic_infos};

/// Every instance of `model`'s font, built with `head.created` and
/// `head.modified` set to `timestamp` (seconds since the Unix epoch). No
/// fonts when any instance failed; the diagnostics say why.
pub fn build(model: &Model, timestamp: i64) -> (Vec<BuiltFont>, Vec<DiagnosticInfo>) {
    let (fonts, mut diagnostics) = mg_font::build_fonts(&model.hir, &BuildOptions { timestamp });
    diagnostics.sort_by_key(|d| d.primary.span.start);
    (fonts, diagnostic_infos(&model.source, &diagnostics))
}
