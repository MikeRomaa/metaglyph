pub mod ast;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod stable_id;
pub mod syntax_kind;

pub use parser::{Parse, TokenSet, parse};
pub use syntax_kind::{Lang, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// `node`'s span, minus any leading trivia nested inside it. A block node
/// (spec §5.2: `<kind> <name>? (config)? { body }?`) is opened, in the
/// parser, before the whitespace between it and the previous declaration
/// is flushed — the same reason [`ast::Literal::token`] exists — so
/// anchoring a diagnostic directly on `node.text_range()` would underline
/// that leading gap instead of the declaration itself.
pub fn trimmed_range(node: &SyntaxNode) -> std::ops::Range<usize> {
    let full: std::ops::Range<usize> = node.text_range().into();
    let start = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| std::ops::Range::<usize>::from(t.text_range()).start)
        .unwrap_or(full.start);
    start..full.end
}
