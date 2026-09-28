//! `mg_diag::Diagnostic` to LSP (plan 4, L0). One function for every
//! stage, so syntax, HIR, and evaluation diagnostics all reach the editor
//! in the same shape the CLI prints them.

use std::str::FromStr;

use lsp_types::{
    CodeDescription, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    Range, Uri,
};
use mg_diag::Severity;

use crate::line_index::{Encoding, LineIndex};

/// The table every `MGxxxx` code is defined and documented in.
pub const CODE_TABLE_URL: &str =
    "https://github.com/MikeRomaa/metaglyph/blob/main/crates/mg-diag/src/codes.rs";

/// Shown as the diagnostic's origin in the editor.
pub const SOURCE: &str = "mg";

/// The byte span `span` of `text` as an LSP range.
pub fn range(
    index: &LineIndex,
    text: &str,
    span: &std::ops::Range<usize>,
    encoding: Encoding,
) -> Range {
    Range::new(
        index.position(text, span.start, encoding),
        index.position(text, span.end, encoding),
    )
}

/// `diagnostic`, found in the document `uri` whose text is `text`, as an
/// LSP diagnostic:
/// - the code, linked to the code table
/// - the primary label's span as the range, and the message as the message
/// - each secondary label as related information, with its own text
/// - `help` and `note` lines appended to the message, as the CLI prints them
pub fn to_lsp(
    diagnostic: &mg_diag::Diagnostic,
    uri: &Uri,
    text: &str,
    index: &LineIndex,
    encoding: Encoding,
) -> lsp_types::Diagnostic {
    let mut message = diagnostic.message.clone();
    for help in &diagnostic.help {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }
    for note in &diagnostic.note {
        message.push_str("\nnote: ");
        message.push_str(note);
    }

    let related: Vec<DiagnosticRelatedInformation> = diagnostic
        .secondary
        .iter()
        .map(|label| DiagnosticRelatedInformation {
            location: Location::new(uri.clone(), range(index, text, &label.span, encoding)),
            message: label.message.clone(),
        })
        .collect();

    lsp_types::Diagnostic {
        range: range(index, text, &diagnostic.primary.span, encoding),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(diagnostic.code.as_str().to_string())),
        code_description: Some(CodeDescription {
            href: Uri::from_str(CODE_TABLE_URL).expect("the code table URL parses"),
        }),
        source: Some(SOURCE.to_string()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        tags: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_diag::{codes, Label};

    fn uri() -> Uri {
        Uri::from_str("file:///font.mg").unwrap()
    }

    #[test]
    fn every_part_of_a_diagnostic_is_mapped() {
        let text = "glyph é (x: 1\n) {";
        let index = LineIndex::new(text);
        let opener = text.find('(').unwrap();
        let diagnostic = mg_diag::Diagnostic::error(
            codes::UNCLOSED_DELIMITER,
            "unclosed `{`",
            Label::new(text.len() - 1..text.len(), "opened here"),
        )
        .with_secondary(Label::new(opener..opener + 1, "earlier `(`"))
        .with_help("add a `}`")
        .with_note("a body must close");

        let lsp = to_lsp(&diagnostic, &uri(), text, &index, Encoding::Utf16);
        assert_eq!(lsp.range.start, lsp_types::Position::new(1, 2));
        assert_eq!(lsp.range.end, lsp_types::Position::new(1, 3));
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.code, Some(NumberOrString::String("MG0101".into())));
        assert_eq!(lsp.code_description.unwrap().href.as_str(), CODE_TABLE_URL);
        assert_eq!(lsp.source.as_deref(), Some("mg"));
        assert_eq!(
            lsp.message,
            "unclosed `{`\nhelp: add a `}`\nnote: a body must close"
        );

        let related = lsp.related_information.unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].message, "earlier `(`");
        assert_eq!(related[0].location.uri, uri());
        // `é` is one UTF-16 unit: the `(` is at column 8, not 9.
        assert_eq!(
            related[0].location.range.start,
            lsp_types::Position::new(0, 8)
        );
    }

    #[test]
    fn a_warning_stays_a_warning_and_no_related_means_none() {
        let text = "x";
        let diagnostic = mg_diag::Diagnostic::warning(
            codes::UNEXPECTED_TOKEN,
            "just a warning",
            Label::new(0..1, "here"),
        );
        let lsp = to_lsp(
            &diagnostic,
            &uri(),
            text,
            &LineIndex::new(text),
            Encoding::Utf8,
        );
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(lsp.related_information, None);
        assert_eq!(lsp.message, "just a warning");
    }
}
