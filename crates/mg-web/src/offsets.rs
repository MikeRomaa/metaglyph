use std::ops::Range;

/// Converts byte offsets into `text` to UTF-16 offsets. Offsets are
/// expected in ascending order for speed, but any order is correct.
pub struct Utf16Map<'a> {
    text: &'a str,
    byte: usize,
    utf16: usize,
}

impl<'a> Utf16Map<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            byte: 0,
            utf16: 0,
        }
    }

    pub fn convert(&mut self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        if byte < self.byte {
            self.byte = 0;
            self.utf16 = 0;
        }
        for c in self.text[self.byte..].chars() {
            if self.byte + c.len_utf8() > byte {
                break;
            }
            self.byte += c.len_utf8();
            self.utf16 += c.len_utf16();
        }
        self.utf16
    }
}

/// A byte → UTF-16 table for random-order lookups, built once per text.
pub struct Utf16Index {
    /// UTF-16 offset of each byte offset (bytes inside a character map to
    /// the character's start).
    table: Vec<u32>,
}

impl Utf16Index {
    pub fn new(text: &str) -> Self {
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut utf16 = 0u32;
        for c in text.chars() {
            for _ in 0..c.len_utf8() {
                table.push(utf16);
            }
            utf16 += c.len_utf16() as u32;
        }
        table.push(utf16);
        Self { table }
    }

    pub fn convert(&self, byte: usize) -> usize {
        self.table[byte.min(self.table.len() - 1)] as usize
    }

    /// The byte offset of UTF-16 offset `utf16` (the start of the
    /// character it falls in).
    pub fn to_byte(&self, utf16: usize) -> usize {
        self.table.partition_point(|&u| (u as usize) < utf16)
    }

    /// `range` as a `[from, to]` UTF-16 pair.
    pub fn span(&self, range: &Range<usize>) -> [usize; 2] {
        [self.convert(range.start), self.convert(range.end)]
    }
}

#[cfg(test)]
mod tests {
    use super::{Utf16Index, Utf16Map};

    #[test]
    fn counts_utf16_units() {
        let text = "a→𝒜b";
        let mut map = Utf16Map::new(text);
        assert_eq!(map.convert(0), 0);
        assert_eq!(map.convert(1), 1);
        assert_eq!(map.convert(4), 2);
        assert_eq!(map.convert(8), 4);
        assert_eq!(map.convert(9), 5);
        assert_eq!(map.convert(1), 1);

        let index = Utf16Index::new(text);
        for byte in [0, 1, 4, 8, 9] {
            assert_eq!(index.convert(byte), Utf16Map::new(text).convert(byte));
        }
        assert_eq!(index.span(&(1..8)), [1, 4]);
        for (utf16, byte) in [(0, 0), (1, 1), (2, 4), (4, 8), (5, 9)] {
            assert_eq!(index.to_byte(utf16), byte);
        }
    }
}
