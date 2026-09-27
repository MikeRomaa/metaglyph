//! Stable node identity (spec §9.3): selection, undo, and diagnostic
//! anchors must survive re-parse, so identity is assigned at parse by
//! structural position, never by byte offset.
//!
//! - A named declaration is keyed by `(kind, name)` under its parent.
//! - An anonymous declaration is keyed by its ordinal among anonymous
//!   siblings of the same kind, so inserting an anonymous sibling
//!   renumbers the later ones. Plan 5 replaces this with tree-diff
//!   matching; until then, ordinals are the whole scheme.

use crate::SyntaxNode;
use crate::syntax_kind::SyntaxKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LocalKey {
    Named(SyntaxKind, String),
    Anonymous(SyntaxKind, usize),
}

/// A node's identity as its path of local keys from the source file root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct NodeId(Vec<LocalKey>);

/// The declaration's own name token, if it has one: the first direct
/// `IDENT` child. Matches the parser's `block()`, which only ever puts an
/// `IDENT` in the name slot.
fn declared_name(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
}

fn local_key(node: &SyntaxNode) -> LocalKey {
    let kind = node.kind();
    if let Some(name) = declared_name(node) {
        return LocalKey::Named(kind, name);
    }
    let ordinal = match node.parent() {
        Some(parent) => parent
            .children()
            .take_while(|sibling| sibling != node)
            .filter(|sibling| sibling.kind() == kind && declared_name(sibling).is_none())
            .count(),
        None => 0,
    };
    LocalKey::Anonymous(kind, ordinal)
}

pub fn node_id(node: &SyntaxNode) -> NodeId {
    let mut keys = Vec::new();
    let mut current = node.clone();
    loop {
        keys.push(local_key(&current));
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    keys.reverse();
    NodeId(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn glyphs(source: &str) -> Vec<SyntaxNode> {
        parse(source)
            .syntax()
            .children()
            .filter(|n| n.kind() == SyntaxKind::GLYPH)
            .collect()
    }

    #[test]
    fn named_siblings_get_distinct_stable_ids() {
        let nodes = glyphs("glyph A (advance: 1) {}\nglyph B (advance: 1) {}\n");
        assert_ne!(node_id(&nodes[0]), node_id(&nodes[1]));
    }

    #[test]
    fn same_named_declaration_has_same_id_regardless_of_source_position() {
        let a = glyphs("glyph A (advance: 1) {}\nglyph B (advance: 1) {}\n");
        let b = glyphs("glyph B (advance: 1) {}\nglyph A (advance: 1) {}\n");
        let id_a_first = node_id(&a[0]); // A, declared first
        let id_a_second = node_id(&b[1]); // A, declared second
        assert_eq!(id_a_first, id_a_second);
    }

    #[test]
    fn anonymous_siblings_renumber_when_one_is_inserted() {
        // `component` is always anonymous (spec §5.7).
        let before = parse("glyph A (advance: 1) { component (glyph: x) component (glyph: y) }\n");
        let after = parse(
            "glyph A (advance: 1) { component (glyph: z) component (glyph: x) component (glyph: y) }\n",
        );
        let comps = |root: &SyntaxNode| -> Vec<SyntaxNode> {
            root.descendants()
                .filter(|n| n.kind() == SyntaxKind::COMPONENT)
                .collect()
        };
        let before_comps = comps(&before.syntax());
        let after_comps = comps(&after.syntax());
        // `x` was ordinal 0, is now ordinal 1: its id changed.
        assert_ne!(node_id(&before_comps[0]), node_id(&after_comps[1]));
    }
}
