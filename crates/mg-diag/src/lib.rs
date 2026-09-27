//! The Metaglyph diagnostic model (spec §13).
//!
//! Designed once, at M0, rather than grown later: rowan supplies spans and
//! `ERROR` nodes but no messages, and every later crate reports errors
//! through this type so the parser can carry labeled spans from its first
//! commit instead of a rewrite once secondary labels are needed.

mod codes;

pub use codes::Code;

use std::ops::Range;

use codespan_reporting::diagnostic::{self, LabelStyle};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A span plus the short claim about it ("unclosed block opened here").
#[derive(Debug, Clone)]
pub struct Label {
    pub span: Range<usize>,
    pub message: String,
}

impl Label {
    pub fn new(span: Range<usize>, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub help: Vec<String>,
    pub note: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: Code, severity: Severity, message: impl Into<String>, primary: Label) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            primary,
            secondary: Vec::new(),
            help: Vec::new(),
            note: Vec::new(),
        }
    }

    pub fn error(code: Code, message: impl Into<String>, primary: Label) -> Self {
        Self::new(code, Severity::Error, message, primary)
    }

    pub fn warning(code: Code, message: impl Into<String>, primary: Label) -> Self {
        Self::new(code, Severity::Warning, message, primary)
    }

    /// Labels a paired construct's opener. Mandatory for unclosed `{`, `(`,
    /// or string errors (spec §13), so the parser must carry the opening
    /// span down the recursion rather than reporting only the failure point.
    pub fn with_secondary(mut self, label: Label) -> Self {
        self.secondary.push(label);
        self
    }

    /// A separate field from `message`, rendered as rustc's `help:` line so
    /// a later editor can turn it into a quick-fix without parsing prose.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note.push(note.into());
        self
    }

    fn to_codespan(&self) -> diagnostic::Diagnostic<()> {
        let severity = match self.severity {
            Severity::Error => diagnostic::Severity::Error,
            Severity::Warning => diagnostic::Severity::Warning,
        };

        let mut labels = vec![
            diagnostic::Label::new(LabelStyle::Primary, (), self.primary.span.clone())
                .with_message(self.primary.message.clone()),
        ];
        labels.extend(self.secondary.iter().map(|label| {
            diagnostic::Label::new(LabelStyle::Secondary, (), label.span.clone())
                .with_message(label.message.clone())
        }));

        let mut notes: Vec<String> = self
            .help
            .iter()
            .map(|help| format!("help: {help}"))
            .collect();
        notes.extend(self.note.iter().map(|note| format!("note: {note}")));

        diagnostic::Diagnostic::new(severity)
            .with_code(self.code.as_str())
            .with_message(self.message.clone())
            .with_labels(labels)
            .with_notes(notes)
    }

    /// Renders this diagnostic as rustc-style text against `source`.
    /// `filename` is shown in the output; it need not exist on disk.
    pub fn render(&self, filename: &str, source: &str) -> String {
        let file = SimpleFile::new(filename, source);
        let diagnostic = self.to_codespan();
        let config = term::Config::default();
        term::emit_into_string(&config, &file, &diagnostic)
            .expect("rendering a diagnostic to an in-memory string does not fail")
    }
}
