use crate::syntax_kind::SyntaxKind;

/// A bitset of `SyntaxKind`s, accumulated by `expect`/`at` helpers and
/// drained into a diagnostic on failure, so a message can read "expected
/// `,` or `)`, found `deg`" instead of "unexpected token".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenSet(u128);

impl TokenSet {
    pub const fn new(kinds: &[SyntaxKind]) -> Self {
        let mut bits = 0u128;
        let mut i = 0;
        while i < kinds.len() {
            bits |= 1u128 << (kinds[i] as u16);
            i += 1;
        }
        TokenSet(bits)
    }

    pub fn contains(self, kind: SyntaxKind) -> bool {
        self.0 & (1u128 << (kind as u16)) != 0
    }

    pub fn iter(self) -> impl Iterator<Item = SyntaxKind> {
        SyntaxKind::ALL
            .iter()
            .copied()
            .filter(move |k| self.contains(*k))
    }
}
