//! Byte offsets (what rowan and `mg-diag` speak) to LSP positions and
//! back. An LSP position is a line plus a column counted in the
//! negotiated encoding: UTF-8 bytes or UTF-16 code units (plan 4, L0).
//! Character literals and comments carry non-ASCII text, so the two
//! differ in real sources.

use lsp_types::Position;

/// How columns are counted, as negotiated at `initialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// The start offset of every line of one document. Lines end at `\n`; a
/// `\r` before it belongs to the line break, not the text, so a column
/// never lands on it.
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            line_starts,
            len: text.len(),
        }
    }

    /// The position of byte `offset` in `text` (the same text the index
    /// was built from). An offset past the end, or inside a multi-byte
    /// character, is clamped back to the nearest character boundary
    /// before it.
    pub fn position(&self, text: &str, offset: usize, encoding: Encoding) -> Position {
        let mut offset = offset.min(self.len);
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let prefix = &text[self.line_starts[line]..offset];
        let prefix = prefix.strip_suffix('\r').unwrap_or(prefix);
        let character = match encoding {
            Encoding::Utf8 => prefix.len(),
            Encoding::Utf16 => prefix.encode_utf16().count(),
        };
        Position::new(line as u32, character as u32)
    }

    /// The byte offset of `position` in `text`. A column past the end of
    /// its line clamps to the line end, and a line past the end of the
    /// document clamps to the document end, as LSP asks.
    pub fn offset(&self, text: &str, position: Position, encoding: Encoding) -> usize {
        let Some(&start) = self.line_starts.get(position.line as usize) else {
            return self.len;
        };
        let end = self
            .line_starts
            .get(position.line as usize + 1)
            .map_or(self.len, |&next| next - 1);
        let line = &text[start..end];
        let line = line.strip_suffix('\r').unwrap_or(line);

        let mut units = 0;
        for (i, ch) in line.char_indices() {
            if units >= position.character as usize {
                return start + i;
            }
            units += match encoding {
                Encoding::Utf8 => ch.len_utf8(),
                Encoding::Utf16 => ch.len_utf16(),
            };
        }
        start + line.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(text: &str, offset: usize, encoding: Encoding, expected: (u32, u32)) {
        let index = LineIndex::new(text);
        let position = index.position(text, offset, encoding);
        assert_eq!((position.line, position.character), expected);
        assert_eq!(index.offset(text, position, encoding), offset);
    }

    #[test]
    fn ascii_is_the_same_in_both_encodings() {
        let text = "font (em: 1000)\nglyph A {}\n";
        for encoding in [Encoding::Utf8, Encoding::Utf16] {
            roundtrip(text, 0, encoding, (0, 0));
            roundtrip(text, 6, encoding, (0, 6));
            roundtrip(text, 16, encoding, (1, 0));
            roundtrip(text, 22, encoding, (1, 6));
            roundtrip(text, text.len(), encoding, (2, 0));
        }
    }

    #[test]
    fn a_character_literal_counts_bytes_or_code_units() {
        // `é` is two UTF-8 bytes and one UTF-16 code unit.
        let text = "let c = 'é'; let d = x;";
        let after = text.find("; let d").unwrap();
        roundtrip(text, after, Encoding::Utf8, (0, 12));
        roundtrip(text, after, Encoding::Utf16, (0, 11));
    }

    #[test]
    fn a_box_drawing_rule_counts_bytes_or_code_units() {
        // `═` is three UTF-8 bytes and one UTF-16 code unit.
        let text = "// ══════\nfont ()";
        let font = text.find("font").unwrap();
        roundtrip(text, font, Encoding::Utf16, (1, 0));
        let index = LineIndex::new(text);
        let end_of_rule = text.find('\n').unwrap();
        assert_eq!(
            index.position(text, end_of_rule, Encoding::Utf8).character,
            21
        );
        assert_eq!(
            index.position(text, end_of_rule, Encoding::Utf16).character,
            9
        );
    }

    #[test]
    fn an_astral_character_is_two_utf16_units() {
        let text = "// 𝔸 x";
        let x = text.find('x').unwrap();
        roundtrip(text, x, Encoding::Utf8, (0, 8));
        roundtrip(text, x, Encoding::Utf16, (0, 6));
    }

    #[test]
    fn crlf_line_breaks_are_not_columns() {
        let text = "a\r\nbc\r\n";
        let index = LineIndex::new(text);
        // The `\r` itself reports as the end of its line.
        assert_eq!(index.position(text, 1, Encoding::Utf8), Position::new(0, 1));
        assert_eq!(index.position(text, 2, Encoding::Utf8), Position::new(0, 1));
        roundtrip(text, 3, Encoding::Utf8, (1, 0));
        assert_eq!(index.offset(text, Position::new(1, 99), Encoding::Utf8), 5);
    }

    #[test]
    fn out_of_range_inputs_clamp() {
        let text = "ab\ncd";
        let index = LineIndex::new(text);
        assert_eq!(
            index.position(text, 99, Encoding::Utf8),
            Position::new(1, 2)
        );
        assert_eq!(index.offset(text, Position::new(0, 99), Encoding::Utf8), 2);
        assert_eq!(index.offset(text, Position::new(9, 0), Encoding::Utf8), 5);
        // Inside `é` clamps back to its start.
        let text = "é";
        let index = LineIndex::new(text);
        assert_eq!(index.position(text, 1, Encoding::Utf8), Position::new(0, 0));
    }
}
