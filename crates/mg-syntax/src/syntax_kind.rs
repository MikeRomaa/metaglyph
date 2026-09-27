/// Every token and node kind in the Metaglyph grammar (spec §5.1–5.2).
///
/// Kept as one flat enum, in the rust-analyzer/rowan style: tokens (leaves,
/// including trivia) and nodes (composites) share one `u16` space so a
/// single `rowan::Language` impl covers both.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Trivia
    WHITESPACE,
    COMMENT,

    // Literals and identifiers. Every number-literal spelling other than
    // the bare decimal form is a `NUMBER_*` subkind (spec §5.1: hex,
    // codepoint, and character integers are all just alternate number
    // spellings, not distinct types).
    IDENT,
    NUMBER,
    NUMBER_ANGLE,
    NUMBER_RATIO,
    NUMBER_HEX,
    NUMBER_CODEPOINT,
    NUMBER_CHAR,
    STRING,

    // Keywords: literals and operator words
    TRUE_KW,
    FALSE_KW,
    AND_KW,
    OR_KW,
    NOT_KW,

    // Keywords: declarations
    FONT_KW,
    PARAM_KW,
    METRIC_KW,
    LET_KW,
    GLYPH_KW,
    INSTANCE_KW,
    GROUP_KW,
    KERN_KW,
    PATH_KW,
    ANCHOR_KW,
    COMPONENT_KW,
    START_KW,
    LINE_KW,
    SPLINE_KW,
    CLOSE_KW,

    // Punctuation
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    L_BRACKET,
    R_BRACKET,
    COMMA,
    SEMICOLON,
    COLON,
    DOT,
    DOTDOT,

    // Operators
    PLUS,
    MINUS,
    STAR,
    SLASH,
    CARET,
    LT,
    LE,
    GT,
    GE,
    EQEQ,
    NEQ,
    EQ,

    // Lexer-level error and end of file
    ERROR_TOKEN,
    EOF,

    // Nodes
    SOURCE_FILE,
    LET_STMT,
    FONT,
    PARAM,
    METRIC,
    GLYPH,
    INSTANCE,
    GROUP,
    KERN,
    PATH,
    ANCHOR,
    COMPONENT,
    START,
    LINE,
    SPLINE,
    CLOSE,
    CONFIG,
    FIELD,
    BODY,
    LITERAL,
    IDENT_EXPR,
    PAREN_EXPR,
    TUPLE_EXPR,
    LIST_EXPR,
    MAP_EXPR,
    MAP_ENTRY,
    UNARY_EXPR,
    BIN_EXPR,
    CALL_EXPR,
    ARG_LIST,
    MEMBER_EXPR,
    RANGE_EXPR,
    ERROR,

    // Kept last so `LAST + 1` is the total count; never matched on directly.
    __LAST,
}

impl SyntaxKind {
    pub(crate) const ALL: &'static [SyntaxKind] = &{
        use SyntaxKind::*;
        [
            WHITESPACE,
            COMMENT,
            IDENT,
            NUMBER,
            NUMBER_ANGLE,
            NUMBER_RATIO,
            NUMBER_HEX,
            NUMBER_CODEPOINT,
            NUMBER_CHAR,
            STRING,
            TRUE_KW,
            FALSE_KW,
            AND_KW,
            OR_KW,
            NOT_KW,
            FONT_KW,
            PARAM_KW,
            METRIC_KW,
            LET_KW,
            GLYPH_KW,
            INSTANCE_KW,
            GROUP_KW,
            KERN_KW,
            PATH_KW,
            ANCHOR_KW,
            COMPONENT_KW,
            START_KW,
            LINE_KW,
            SPLINE_KW,
            CLOSE_KW,
            L_PAREN,
            R_PAREN,
            L_BRACE,
            R_BRACE,
            L_BRACKET,
            R_BRACKET,
            COMMA,
            SEMICOLON,
            COLON,
            DOT,
            DOTDOT,
            PLUS,
            MINUS,
            STAR,
            SLASH,
            CARET,
            LT,
            LE,
            GT,
            GE,
            EQEQ,
            NEQ,
            EQ,
            ERROR_TOKEN,
            EOF,
            SOURCE_FILE,
            LET_STMT,
            FONT,
            PARAM,
            METRIC,
            GLYPH,
            INSTANCE,
            GROUP,
            KERN,
            PATH,
            ANCHOR,
            COMPONENT,
            START,
            LINE,
            SPLINE,
            CLOSE,
            CONFIG,
            FIELD,
            BODY,
            LITERAL,
            IDENT_EXPR,
            PAREN_EXPR,
            TUPLE_EXPR,
            LIST_EXPR,
            MAP_EXPR,
            MAP_ENTRY,
            UNARY_EXPR,
            BIN_EXPR,
            CALL_EXPR,
            ARG_LIST,
            MEMBER_EXPR,
            RANGE_EXPR,
            ERROR,
            __LAST,
        ]
    };

    pub fn from_u16(value: u16) -> Self {
        Self::ALL[value as usize]
    }

    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// Looks up a declaration keyword's text, for the lexer's keyword table.
    pub fn keyword_from_text(text: &str) -> Option<SyntaxKind> {
        use SyntaxKind::*;
        Some(match text {
            "true" => TRUE_KW,
            "false" => FALSE_KW,
            "and" => AND_KW,
            "or" => OR_KW,
            "not" => NOT_KW,
            "font" => FONT_KW,
            "param" => PARAM_KW,
            "metric" => METRIC_KW,
            "let" => LET_KW,
            "glyph" => GLYPH_KW,
            "instance" => INSTANCE_KW,
            "group" => GROUP_KW,
            "kern" => KERN_KW,
            "path" => PATH_KW,
            "anchor" => ANCHOR_KW,
            "component" => COMPONENT_KW,
            "start" => START_KW,
            "line" => LINE_KW,
            "spline" => SPLINE_KW,
            "close" => CLOSE_KW,
            _ => return None,
        })
    }

    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    /// True for `IDENT` and every keyword lexed from identifier-shaped
    /// text. Map-literal keys (`caps: { start: "butt" }`) are reserved
    /// words syntactically, so they must be accepted here even though a
    /// declaration's own name slot deliberately only accepts `IDENT`.
    pub fn is_word(self) -> bool {
        use SyntaxKind::*;
        matches!(
            self,
            IDENT
                | TRUE_KW
                | FALSE_KW
                | AND_KW
                | OR_KW
                | NOT_KW
                | FONT_KW
                | PARAM_KW
                | METRIC_KW
                | LET_KW
                | GLYPH_KW
                | INSTANCE_KW
                | GROUP_KW
                | KERN_KW
                | PATH_KW
                | ANCHOR_KW
                | COMPONENT_KW
                | START_KW
                | LINE_KW
                | SPLINE_KW
                | CLOSE_KW
        )
    }

    /// The declaration-keyword kind that starts a generic block, if any.
    /// Used both by the parser's block dispatch and by its error-recovery
    /// synchronization set (spec-plan M1: "recovery at `;`, `}`, and
    /// declaration-keyword boundaries").
    pub fn starts_declaration(self) -> bool {
        use SyntaxKind::*;
        matches!(
            self,
            LET_KW
                | FONT_KW
                | PARAM_KW
                | METRIC_KW
                | GLYPH_KW
                | INSTANCE_KW
                | GROUP_KW
                | KERN_KW
                | PATH_KW
                | ANCHOR_KW
                | COMPONENT_KW
                | START_KW
                | LINE_KW
                | SPLINE_KW
                | CLOSE_KW
        )
    }

    /// A short, human-facing spelling for diagnostics ("expected `,` or
    /// `)`, found `deg`").
    pub fn describe(self) -> &'static str {
        use SyntaxKind::*;
        match self {
            WHITESPACE => "whitespace",
            COMMENT => "a comment",
            IDENT => "an identifier",
            NUMBER => "a number",
            NUMBER_ANGLE => "an angle",
            NUMBER_RATIO => "a ratio",
            NUMBER_HEX => "a hex number",
            NUMBER_CODEPOINT => "a codepoint",
            NUMBER_CHAR => "a character literal",
            STRING => "a string",
            TRUE_KW => "`true`",
            FALSE_KW => "`false`",
            AND_KW => "`and`",
            OR_KW => "`or`",
            NOT_KW => "`not`",
            FONT_KW => "`font`",
            PARAM_KW => "`param`",
            METRIC_KW => "`metric`",
            LET_KW => "`let`",
            GLYPH_KW => "`glyph`",
            INSTANCE_KW => "`instance`",
            GROUP_KW => "`group`",
            KERN_KW => "`kern`",
            PATH_KW => "`path`",
            ANCHOR_KW => "`anchor`",
            COMPONENT_KW => "`component`",
            START_KW => "`start`",
            LINE_KW => "`line`",
            SPLINE_KW => "`spline`",
            CLOSE_KW => "`close`",
            L_PAREN => "`(`",
            R_PAREN => "`)`",
            L_BRACE => "`{`",
            R_BRACE => "`}`",
            L_BRACKET => "`[`",
            R_BRACKET => "`]`",
            COMMA => "`,`",
            SEMICOLON => "`;`",
            COLON => "`:`",
            DOT => "`.`",
            DOTDOT => "`..`",
            PLUS => "`+`",
            MINUS => "`-`",
            STAR => "`*`",
            SLASH => "`/`",
            CARET => "`^`",
            LT => "`<`",
            LE => "`<=`",
            GT => "`>`",
            GE => "`>=`",
            EQEQ => "`==`",
            NEQ => "`!=`",
            EQ => "`=`",
            ERROR_TOKEN => "an unrecognized character",
            EOF => "end of file",
            _ => "a token",
        }
    }
}

/// The Metaglyph rowan language. Uninhabited: it exists only to carry the
/// `Language` impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}

impl rowan::Language for Lang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_u16(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.to_u16())
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Lang>;
pub type SyntaxToken = rowan::SyntaxToken<Lang>;
pub type SyntaxElement = rowan::NodeOrToken<SyntaxNode, SyntaxToken>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_u16() {
        for (i, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(kind.to_u16(), i as u16);
            assert_eq!(SyntaxKind::from_u16(i as u16), *kind);
        }
    }
}
