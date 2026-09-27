mod grammar;
mod token_set;

pub use token_set::TokenSet;

use crate::lexer::RawToken;
use crate::syntax_kind::{Lang, SyntaxKind};
use mg_diag::codes;
use mg_diag::{Diagnostic, Label};
use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, Language};

pub struct Parse {
    green: GreenNode,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parse {
    pub fn syntax(&self) -> crate::SyntaxNode {
        crate::SyntaxNode::new_root(self.green.clone())
    }
}

pub fn parse(source: &str) -> Parse {
    let (tokens, lex_diagnostics) = crate::lexer::tokenize(source);
    let mut parser = Parser {
        source,
        tokens,
        pos: 0,
        builder: GreenNodeBuilder::new(),
        diagnostics: lex_diagnostics,
    };
    grammar::source_file(&mut parser);
    parser.diagnostics.sort_by_key(|d| d.primary.span.start);
    Parse {
        green: parser.builder.finish(),
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<RawToken>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    /// The kind of the `n`th significant (non-trivia) token ahead; `n = 0`
    /// is the current token. Always `EOF` past the end of the stream.
    fn nth(&self, n: usize) -> SyntaxKind {
        let mut seen = 0;
        let mut idx = self.pos;
        loop {
            match self.tokens.get(idx) {
                Some(t) if t.kind.is_trivia() => idx += 1,
                Some(t) => {
                    if seen == n {
                        return t.kind;
                    }
                    seen += 1;
                    idx += 1;
                }
                None => return SyntaxKind::EOF,
            }
        }
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == kind
    }

    fn at_ident_like(&self) -> bool {
        matches!(
            self.nth(0),
            SyntaxKind::IDENT
                | SyntaxKind::GLYPH_KW
                | SyntaxKind::INSTANCE_KW
                | SyntaxKind::FONT_KW
        )
    }

    /// Byte range of the current significant token (a zero-width range at
    /// EOF), for anchoring a diagnostic label.
    fn current_span(&self) -> std::ops::Range<usize> {
        let mut idx = self.pos;
        loop {
            match self.tokens.get(idx) {
                Some(t) if t.kind.is_trivia() => idx += 1,
                Some(t) => return t.start..t.end,
                None => {
                    let end = self.source.len();
                    return end..end;
                }
            }
        }
    }

    fn bump_trivia(&mut self) {
        while let Some(t) = self.tokens.get(self.pos) {
            if !t.kind.is_trivia() {
                break;
            }
            self.builder
                .token(Lang::kind_to_raw(t.kind), t.text(self.source));
            self.pos += 1;
        }
    }

    /// Flushes pending trivia, then consumes and emits the current
    /// significant token (or a zero-width `EOF` past the end).
    fn bump(&mut self) -> SyntaxKind {
        self.bump_trivia();
        let idx = self.pos.min(self.tokens.len() - 1);
        let t = self.tokens[idx];
        self.builder
            .token(Lang::kind_to_raw(t.kind), t.text(self.source));
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t.kind
    }

    /// For a paired construct's closing delimiter:
    /// reports `UNCLOSED_DELIMITER` with a secondary label at the opener
    /// (`open_span`), so "unclosed `{`" points at the `{` even when EOF is
    /// hundreds of lines away. Every call site captures `open_span` via
    /// `current_span()` immediately before bumping the opening token.
    fn expect_closing(&mut self, kind: SyntaxKind, open_span: std::ops::Range<usize>) -> bool {
        if self.at(kind) {
            self.bump();
            return true;
        }
        let opener = match kind {
            SyntaxKind::R_PAREN => "(",
            SyntaxKind::R_BRACE => "{",
            SyntaxKind::R_BRACKET => "[",
            _ => unreachable!("expect_closing is only called for delimiter kinds"),
        };
        let span = self.current_span();
        let found = self.found_description();
        self.diagnostics.push(
            Diagnostic::error(
                codes::UNCLOSED_DELIMITER,
                format!("expected {}, found {found}", kind.describe()),
                Label::new(span, format!("found {found}")),
            )
            .with_secondary(Label::new(
                open_span,
                format!("unclosed `{opener}` opened here"),
            )),
        );
        false
    }

    fn error_expected(&mut self, expected: TokenSet) {
        let span = self.current_span();
        let found = self.found_description();
        let message = format_expected(expected, &found);
        self.diagnostics.push(Diagnostic::error(
            codes::UNEXPECTED_TOKEN,
            message,
            Label::new(span, format!("found {found}")),
        ));
    }

    /// A backtick-quoted spelling of the current token for diagnostics:
    /// its literal source text for variable-spelling tokens (idents,
    /// numbers, strings), otherwise its fixed `describe()` spelling.
    fn found_description(&self) -> String {
        let kind = self.nth(0);
        match kind {
            SyntaxKind::EOF => "end of file".to_string(),
            SyntaxKind::IDENT
            | SyntaxKind::NUMBER
            | SyntaxKind::NUMBER_ANGLE
            | SyntaxKind::NUMBER_RATIO
            | SyntaxKind::NUMBER_HEX
            | SyntaxKind::NUMBER_CODEPOINT
            | SyntaxKind::NUMBER_CHAR
            | SyntaxKind::STRING => {
                let span = self.current_span();
                format!("`{}`", &self.source[span])
            }
            _ => kind.describe().to_string(),
        }
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(Lang::kind_to_raw(kind));
    }

    fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    fn checkpoint(&self) -> Checkpoint {
        self.builder.checkpoint()
    }

    fn start_node_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.builder
            .start_node_at(checkpoint, Lang::kind_to_raw(kind));
    }
}

fn format_expected(expected: TokenSet, found: &str) -> String {
    let items: Vec<&str> = expected.iter().map(SyntaxKind::describe).collect();
    let expected_str = match items.as_slice() {
        [] => "something else".to_string(),
        [only] => (*only).to_string(),
        [first, second] => format!("{first} or {second}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    };
    format!("expected {expected_str}, found {found}")
}
