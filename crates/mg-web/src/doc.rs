use indexmap::IndexMap;
use mg_diag::Severity;
use mg_syntax::ast::AstNode;
use serde::Serialize;

use crate::offsets::Utf16Map;

/// Everything the shell shows about a document (plan 6, W1).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocState {
    pub version: u32,
    /// `false` when the text has syntax errors: the editor then keeps the
    /// last good scene and makes the canvas read-only (plan 5, §1.1).
    pub parse_ok: bool,
    pub diagnostics: Vec<DiagnosticInfo>,
    pub font: Option<FontInfo>,
    pub instances: Vec<String>,
    pub glyph_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInfo {
    /// UTF-16 offsets.
    pub from: usize,
    pub to: usize,
    pub severity: &'static str,
    pub code: &'static str,
    /// The message with its `help` and `note` lines, as the CLI prints them.
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub name: Option<String>,
    pub version: String,
    pub designer: Option<String>,
    pub foundry: Option<String>,
    pub em: Option<i64>,
}

/// Parses, lowers and evaluates `source` in every instance, the way
/// `mg check` does plus evaluation (as the LSP does).
pub fn check(source: &str, version: u32) -> DocState {
    let parsed = mg_syntax::parse(source);
    let parse_ok = !parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    let mut state = DocState {
        version,
        parse_ok,
        diagnostics: Vec::new(),
        font: None,
        instances: Vec::new(),
        glyph_count: 0,
    };

    let mut diagnostics = parsed.diagnostics.clone();
    // Same reason as `mg check`: recovery from a syntax error can misplace
    // whole declarations, and the HIR errors that follow only restate it.
    if parse_ok {
        let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (hir, hir_diagnostics) = mg_hir::lower(&source_file);
        diagnostics.extend(hir_diagnostics);
        diagnostics.extend(mg_font::build::check_codepoints(&hir));
        diagnostics.extend(evaluation_diagnostics(&hir));

        state.font = Some(FontInfo {
            name: hir.font.name.clone(),
            version: hir.font.version.clone(),
            designer: hir.font.designer.clone(),
            foundry: hir.font.foundry.clone(),
            em: hir.font.em,
        });
        state.instances = hir.instances.keys().cloned().collect();
        state.glyph_count = hir.glyphs.len();
    }

    diagnostics.sort_by_key(|d| d.primary.span.start);
    let mut map = Utf16Map::new(source);
    state.diagnostics = diagnostics
        .iter()
        .map(|d| {
            let mut message = d.message.clone();
            for help in &d.help {
                message.push_str("\nhelp: ");
                message.push_str(help);
            }
            for note in &d.note {
                message.push_str("\nnote: ");
                message.push_str(note);
            }
            DiagnosticInfo {
                from: map.convert(d.primary.span.start),
                to: map.convert(d.primary.span.end),
                severity: match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                code: d.code.as_str(),
                message,
            }
        })
        .collect();
    state
}

/// Every instance's evaluation diagnostics, each distinct one once, with the
/// instances it fired in appended to its message. Mirrors
/// `mg_lsp::evaluation::diagnostics`, which can't be used here because
/// mg-lsp depends on `lsp-server`.
fn evaluation_diagnostics(hir: &mg_hir::Hir) -> Vec<mg_diag::Diagnostic> {
    type Key = (&'static str, String, std::ops::Range<usize>);
    let mut merged: IndexMap<Key, (mg_diag::Diagnostic, Vec<&str>)> = IndexMap::new();
    for instance in hir.instances.values() {
        let (_, outcome) = mg_eval::evaluate(hir, instance);
        for diagnostic in outcome.diagnostics {
            let key = (
                diagnostic.code.as_str(),
                diagnostic.message.clone(),
                diagnostic.primary.span.clone(),
            );
            merged
                .entry(key)
                .or_insert_with(|| (diagnostic, Vec::new()))
                .1
                .push(&instance.name);
        }
    }
    let many = hir.instances.len() > 1;
    merged
        .into_values()
        .map(|(mut diagnostic, instances)| {
            if many {
                diagnostic.message = format!("{} [{}]", diagnostic.message, instances.join(", "));
            }
            diagnostic
        })
        .collect()
}
