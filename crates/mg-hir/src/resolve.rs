//! Reserved words and the value namespaces (spec §5.4, §5.11 rules 3, 4,
//! and 6). The glyph namespace, glyph-set namespace, and segment namespace are
//! simple enough (existence checks over an already-built `IndexMap`) that
//! [`crate::lower`] checks them directly; this module owns the one
//! namespace shape that repeats — top-level scope and each glyph's own
//! scope are both a flat set of names with the same declare-time rules.

use std::ops::Range;

use indexmap::IndexMap;
use mg_diag::{Diagnostic, Label};

use mg_diag::codes;

/// The closed reserved-word list (spec §5.4): declaration keywords,
/// built-in constants, literals, operator words, and namespace roots.
/// None of these may name a declaration in any namespace (spec §5.11 rule
/// 6). Most are already unreachable as a declaration name because the
/// parser's name slot accepts only a plain `IDENT` token and these lex as
/// their own keyword — `math`, `glyphs`, and the built-in constants have
/// no keyword of their own, so they are the ones this list actually
/// catches in practice.
pub const RESERVED_WORDS: &[&str] = &[
    "font",
    "param",
    "metric",
    "let",
    "glyph",
    "instance",
    "group",
    "kern",
    "path",
    "anchor",
    "component",
    "start",
    "line",
    "quad",
    "cube",
    "arc",
    "close",
    "up",
    "down",
    "left",
    "right",
    "identity",
    "true",
    "false",
    "and",
    "or",
    "not",
    "math",
    "glyphs",
];

pub fn is_reserved(name: &str) -> bool {
    RESERVED_WORDS.contains(&name)
}

/// A flat value namespace (spec §5.11: top-level scope, or one glyph's
/// scope), enforcing "no reserved word," "no duplicate," and — when an
/// outer namespace is supplied at `declare` time — "no shadowing."
#[derive(Debug, Default)]
pub struct ValueNamespace {
    names: IndexMap<String, Range<usize>>,
}

impl ValueNamespace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares `name` at `span`. Reports and refuses a reserved word or a
    /// duplicate within this namespace; reports but still admits a name
    /// that shadows `outer` (spec §5.11 rule 3 makes shadowing an error,
    /// not a reason to skip binding the inner name — later lookups still
    /// resolve to the inner declaration).
    pub fn declare(
        &mut self,
        name: &str,
        span: Range<usize>,
        outer: Option<&ValueNamespace>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        if is_reserved(name) {
            diagnostics.push(Diagnostic::error(
                codes::RESERVED_WORD_NAME,
                format!("`{name}` is a reserved word and cannot be a declaration name"),
                Label::new(span, "reserved word"),
            ));
            return false;
        }

        if let Some(first) = self.names.get(name) {
            diagnostics.push(
                Diagnostic::error(
                    codes::DUPLICATE_DEFINITION,
                    format!("`{name}` is already defined in this scope"),
                    Label::new(span, "duplicate definition"),
                )
                .with_secondary(Label::new(first.clone(), "first defined here")),
            );
            return false;
        }

        if let Some(outer) = outer
            && let Some(outer_span) = outer.names.get(name)
        {
            diagnostics.push(
                Diagnostic::error(
                    codes::SHADOWS_TOP_LEVEL,
                    format!("`{name}` shadows the top-level name `{name}`"),
                    Label::new(span.clone(), "shadows a top-level name"),
                )
                .with_secondary(Label::new(
                    outer_span.clone(),
                    "top-level name defined here",
                )),
            );
        }

        self.names.insert(name.to_string(), span);
        true
    }
}

/// Builds an "unresolved identifier" diagnostic with a near-miss
/// suggestion drawn from `candidates`, when one is close enough (spec
/// §13: "Unresolved identifier with scope and near-miss suggestions").
pub fn unresolved_name<'a>(
    name: &str,
    span: Range<usize>,
    scope_description: &str,
    candidates: impl Iterator<Item = &'a str> + Clone,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(
        codes::UNRESOLVED_NAME,
        format!("cannot find `{name}` in {scope_description}"),
        Label::new(span, "not found"),
    );
    match mg_diag::suggest::nearest_match(name, candidates) {
        Some(suggestion) => {
            diagnostic.with_help(format!("a similarly named value exists: `{suggestion}`"))
        }
        None => diagnostic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reserved_word() {
        let mut ns = ValueNamespace::new();
        let mut diags = Vec::new();
        assert!(!ns.declare("glyphs", 0..1, None, &mut diags));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, codes::RESERVED_WORD_NAME);
    }

    #[test]
    fn rejects_duplicate() {
        let mut ns = ValueNamespace::new();
        let mut diags = Vec::new();
        assert!(ns.declare("w", 0..1, None, &mut diags));
        assert!(!ns.declare("w", 5..6, None, &mut diags));
        assert_eq!(diags[0].code, codes::DUPLICATE_DEFINITION);
    }

    #[test]
    fn reports_but_admits_shadowing() {
        let mut outer = ValueNamespace::new();
        let mut diags = Vec::new();
        outer.declare("stem", 0..1, None, &mut diags);

        let mut inner = ValueNamespace::new();
        assert!(inner.declare("stem", 10..11, Some(&outer), &mut diags));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, codes::SHADOWS_TOP_LEVEL);
    }
}
