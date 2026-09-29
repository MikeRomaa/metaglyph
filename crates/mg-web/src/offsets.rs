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

#[cfg(test)]
mod tests {
    use super::Utf16Map;

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
    }
}
