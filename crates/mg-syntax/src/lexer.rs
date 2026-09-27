use crate::syntax_kind::SyntaxKind;
use mg_diag::codes;
use mg_diag::{Diagnostic, Label};

/// One lexed token: a kind plus its byte range in the source. Trivia
/// (`WHITESPACE`, `COMMENT`) is included, in order, so the parser can
/// reconstruct the source byte-for-byte from the token stream alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawToken {
    pub kind: SyntaxKind,
    pub start: usize,
    pub end: usize,
}

impl RawToken {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

pub struct Lexer<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    pos: usize,
    tokens: Vec<RawToken>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().collect(),
            pos: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn tokenize(mut self) -> (Vec<RawToken>, Vec<Diagnostic>) {
        while let Some((start, c)) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' => self.lex_whitespace(),
                '/' if self.peek_at(1) == Some('/') => self.lex_comment(),
                '"' => self.lex_string(start),
                '\'' => self.lex_char(start),
                '0'..='9' => self.lex_number(),
                c if is_ident_start(c) => self.lex_ident_or_codepoint(start),
                _ => self.lex_punct(start, c),
            }
        }

        self.tokens.push(RawToken {
            kind: SyntaxKind::EOF,
            start: self.source.len(),
            end: self.source.len(),
        });

        (self.tokens, self.diagnostics)
    }

    fn peek(&self) -> Option<(usize, char)> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).map(|&(_, c)| c)
    }

    fn byte_offset_at(&self, index: usize) -> usize {
        self.chars
            .get(index)
            .map(|&(b, _)| b)
            .unwrap_or(self.source.len())
    }

    fn push(&mut self, kind: SyntaxKind, start: usize, end: usize) {
        self.tokens.push(RawToken { kind, start, end });
    }

    fn lex_whitespace(&mut self) {
        let start = self.byte_offset_at(self.pos);

        while matches!(self.peek(), Some((_, ' ' | '\t' | '\r' | '\n'))) {
            self.pos += 1;
        }

        let end = self.byte_offset_at(self.pos);
        self.push(SyntaxKind::WHITESPACE, start, end);
    }

    fn lex_comment(&mut self) {
        let start = self.byte_offset_at(self.pos);

        while matches!(self.peek(), Some((_, c)) if c != '\n') {
            self.pos += 1;
        }

        let end = self.byte_offset_at(self.pos);
        self.push(SyntaxKind::COMMENT, start, end);
    }

    fn lex_string(&mut self, start: usize) {
        self.pos += 1; // '"'

        let mut terminated = false;

        while let Some((_, c)) = self.peek() {
            match c {
                '"' => {
                    self.pos += 1;
                    terminated = true;
                    break;
                }
                '\n' => break,
                '\\' => {
                    let escape_start = self.byte_offset_at(self.pos);
                    self.pos += 1;
                    match self.peek() {
                        Some((_, escaped)) => {
                            if !matches!(escaped, '"' | '\\' | 'n' | 't') {
                                let escape_end = self.byte_offset_at(self.pos + 1);
                                self.diagnostics.push(Diagnostic::error(
                                    codes::UNKNOWN_ESCAPE,
                                    format!("unknown escape sequence `\\{escaped}`"),
                                    Label::new(escape_start..escape_end, "unknown escape"),
                                ));
                            }
                            self.pos += 1;
                        }
                        None => break,
                    }
                }
                _ => self.pos += 1,
            }
        }

        let end = self.byte_offset_at(self.pos);
        self.push(SyntaxKind::STRING, start, end);

        if !terminated {
            self.diagnostics.push(Diagnostic::error(
                codes::UNTERMINATED_STRING,
                "unterminated string literal",
                Label::new(start..end, "string starts here"),
            ));
        }
    }

    /// A character integer literal (spec §5.1): `'` exactly one Unicode
    /// scalar value (or one of `\'` `\\` `\n` `\t`) `'`. Its value is the
    /// scalar's codepoint — a plain `num`, same as `65`, `0x41`, or
    /// `U+0041`; unlike a string, more than one scalar between the quotes
    /// is an error rather than a longer literal (a decomposed `é` is two
    /// scalars).
    fn lex_char(&mut self, start: usize) {
        self.pos += 1; // opening '

        let mut scalar_count = 0usize;
        let mut terminated = false;

        while let Some((_, c)) = self.peek() {
            match c {
                '\'' => {
                    self.pos += 1;
                    terminated = true;
                    break;
                }
                '\n' => break,
                '\\' => {
                    let escape_start = self.byte_offset_at(self.pos);
                    self.pos += 1;
                    match self.peek() {
                        Some((_, escaped)) => {
                            if !matches!(escaped, '\'' | '\\' | 'n' | 't') {
                                let escape_end = self.byte_offset_at(self.pos + 1);
                                self.diagnostics.push(Diagnostic::error(
                                    codes::UNKNOWN_ESCAPE,
                                    format!("unknown escape sequence `\\{escaped}`"),
                                    Label::new(escape_start..escape_end, "unknown escape"),
                                ));
                            }
                            self.pos += 1;
                            scalar_count += 1;
                        }
                        None => break,
                    }
                }
                _ => {
                    self.pos += 1;
                    scalar_count += 1;
                }
            }
        }

        let end = self.byte_offset_at(self.pos);
        self.push(SyntaxKind::NUMBER_CHAR, start, end);

        if !terminated {
            self.diagnostics.push(Diagnostic::error(
                codes::UNTERMINATED_CHAR,
                "unterminated character literal",
                Label::new(start..end, "character literal starts here"),
            ));
        } else if scalar_count == 0 {
            self.diagnostics.push(Diagnostic::error(
                codes::EMPTY_CHAR_LITERAL,
                "character literal is empty",
                Label::new(start..end, "empty character literal"),
            ));
        } else if scalar_count > 1 {
            self.diagnostics.push(Diagnostic::error(
                codes::CHAR_LITERAL_MULTIPLE_SCALARS,
                "character literal holds more than one scalar value",
                Label::new(start..end, "expected exactly one scalar value"),
            ));
        }
    }

    fn lex_number(&mut self) {
        let start = self.byte_offset_at(self.pos);

        if self.peek().map(|(_, c)| c) == Some('0')
            && self.peek_at(1) == Some('x')
            && let Some(token) = self.try_lex_hex(start)
        {
            self.tokens.push(token);
            return;
        }

        while matches!(self.peek(), Some((_, c)) if c.is_ascii_digit()) {
            self.pos += 1;
        }

        if self.peek().map(|(_, c)| c) == Some('.')
            && self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
        {
            self.pos += 1; // '.'
            while matches!(self.peek(), Some((_, c)) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }

        let kind = self.try_lex_suffix().unwrap_or(SyntaxKind::NUMBER);
        let end = self.byte_offset_at(self.pos);
        self.push(kind, start, end);
    }

    /// Attempts to lex `0x` followed by one or more hex digits. Returns
    /// `None` (consuming nothing) when no hex digit follows, so the caller
    /// falls back to lexing a bare `0` and a separate `x...` identifier —
    /// the same backing-off shape as `try_lex_codepoint`. Hex integers
    /// never take a suffix (spec §5.1).
    fn try_lex_hex(&mut self, start: usize) -> Option<RawToken> {
        let mut hex_len = 0;
        while self
            .peek_at(2 + hex_len)
            .is_some_and(|c| c.is_ascii_hexdigit())
        {
            hex_len += 1;
        }
        if hex_len == 0 {
            return None;
        }
        let digits_start = self.byte_offset_at(self.pos + 2);
        self.pos += 2 + hex_len;
        let end = self.byte_offset_at(self.pos);
        let hex_text = &self.source[digits_start..end];
        let too_large = match u64::from_str_radix(hex_text, 16) {
            Ok(value) => value > (1u64 << 53),
            Err(_) => true,
        };
        if too_large {
            self.diagnostics.push(Diagnostic::error(
                codes::HEX_INTEGER_OUT_OF_RANGE,
                format!("hex integer 0x{hex_text} exceeds 2^53"),
                Label::new(start..end, "hex integer out of range"),
            ));
        }
        Some(RawToken {
            kind: SyntaxKind::NUMBER_HEX,
            start,
            end,
        })
    }

    /// Consumes a `deg`/`rad`/`em`/`%` suffix immediately following a
    /// number, with no intervening whitespace, and returns the resulting
    /// subkind: `NUMBER_ANGLE` for `deg`/`rad`, `NUMBER_RATIO` for `em`/`%`.
    /// Word suffixes must not be followed by another identifier character,
    /// so `100em2` does not consume `em` as a suffix.
    fn try_lex_suffix(&mut self) -> Option<SyntaxKind> {
        if self.peek().map(|(_, c)| c) == Some('%') {
            self.pos += 1;
            return Some(SyntaxKind::NUMBER_RATIO);
        }

        for (suffix, kind) in [
            ("deg", SyntaxKind::NUMBER_ANGLE),
            ("rad", SyntaxKind::NUMBER_ANGLE),
            ("em", SyntaxKind::NUMBER_RATIO),
        ] {
            let len = suffix.chars().count();
            let matches = suffix
                .chars()
                .enumerate()
                .all(|(i, expected)| self.peek_at(i) == Some(expected));

            if matches {
                let boundary_ok = match self.peek_at(len) {
                    Some(c) => !is_ident_continue(c),
                    None => true,
                };
                if boundary_ok {
                    self.pos += len;
                    return Some(kind);
                }
            }
        }

        None
    }

    fn lex_ident_or_codepoint(&mut self, start: usize) {
        if self.peek().map(|(_, c)| c) == Some('U')
            && self.peek_at(1) == Some('+')
            && let Some(token) = self.try_lex_codepoint(start)
        {
            self.tokens.push(token);
            return;
        }

        while matches!(self.peek(), Some((_, c)) if is_ident_continue(c)) {
            self.pos += 1;
        }

        let end = self.byte_offset_at(self.pos);
        let kind =
            SyntaxKind::keyword_from_text(&self.source[start..end]).unwrap_or(SyntaxKind::IDENT);
        self.push(kind, start, end);
    }

    /// Attempts to lex `U+` followed by 4–6 hex digits. Returns `None`
    /// (consuming nothing) when fewer than 4 hex digits follow, so the
    /// caller falls back to lexing `U` as a plain identifier.
    fn try_lex_codepoint(&mut self, start: usize) -> Option<RawToken> {
        let mut hex_len = 0;
        while hex_len < 6
            && self
                .peek_at(2 + hex_len)
                .is_some_and(|c| c.is_ascii_hexdigit())
        {
            hex_len += 1;
        }
        if hex_len < 4 {
            return None;
        }
        self.pos += 2 + hex_len; // 'U' '+' plus the hex digits
        let end = self.byte_offset_at(self.pos);
        let hex_text = &self.source[start + 2..end];
        if let Ok(value) = u32::from_str_radix(hex_text, 16)
            && value > 0x10FFFF
        {
            self.diagnostics.push(Diagnostic::error(
                codes::INVALID_CODEPOINT,
                format!("codepoint U+{hex_text} exceeds U+10FFFF"),
                Label::new(start..end, "codepoint out of range"),
            ));
        }
        Some(RawToken {
            kind: SyntaxKind::NUMBER_CODEPOINT,
            start,
            end,
        })
    }

    /// Classifies a punctuation or operator character starting at the
    /// current position. `Err` means `c` isn't one of these, leaving the
    /// caller to report it rather than guessing a token kind for it.
    fn classify_punct(&self, c: char) -> Option<(SyntaxKind, usize)> {
        let next = |second: char| self.peek_at(1) == Some(second);
        Some(match c {
            '(' => (SyntaxKind::L_PAREN, 1),
            ')' => (SyntaxKind::R_PAREN, 1),
            '{' => (SyntaxKind::L_BRACE, 1),
            '}' => (SyntaxKind::R_BRACE, 1),
            '[' => (SyntaxKind::L_BRACKET, 1),
            ']' => (SyntaxKind::R_BRACKET, 1),
            ',' => (SyntaxKind::COMMA, 1),
            ';' => (SyntaxKind::SEMICOLON, 1),
            ':' => (SyntaxKind::COLON, 1),
            '.' if next('.') => (SyntaxKind::DOTDOT, 2),
            '.' => (SyntaxKind::DOT, 1),
            '+' => (SyntaxKind::PLUS, 1),
            '-' => (SyntaxKind::MINUS, 1),
            '*' => (SyntaxKind::STAR, 1),
            '/' => (SyntaxKind::SLASH, 1),
            '^' => (SyntaxKind::CARET, 1),
            '<' if next('=') => (SyntaxKind::LE, 2),
            '<' => (SyntaxKind::LT, 1),
            '>' if next('=') => (SyntaxKind::GE, 2),
            '>' => (SyntaxKind::GT, 1),
            '=' if next('=') => (SyntaxKind::EQEQ, 2),
            '=' => (SyntaxKind::EQ, 1),
            '!' if next('=') => (SyntaxKind::NEQ, 2),
            _ => return None,
        })
    }

    fn lex_punct(&mut self, start: usize, c: char) {
        match self.classify_punct(c) {
            Some((kind, len)) => {
                self.pos += len;

                let end = self.byte_offset_at(self.pos);
                self.push(kind, start, end);
            }
            None => {
                self.pos += 1;

                let end = self.byte_offset_at(self.pos);
                self.push(SyntaxKind::ERROR_TOKEN, start, end);

                self.diagnostics.push(Diagnostic::error(
                    codes::UNRECOGNIZED_CHARACTER,
                    format!("unrecognized character `{c}`"),
                    Label::new(start..end, "not valid here"),
                ));
            }
        }
    }
}

pub fn tokenize(source: &str) -> (Vec<RawToken>, Vec<Diagnostic>) {
    Lexer::new(source).tokenize()
}
