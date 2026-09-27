//! `MG01xx` — syntax diagnostics (lexer and parser).
use mg_diag::Code;

pub const UNCLOSED_DELIMITER: Code = Code::new("MG0101");
pub const UNEXPECTED_TOKEN: Code = Code::new("MG0102");
pub const INVALID_CODEPOINT: Code = Code::new("MG0103");
pub const UNKNOWN_ESCAPE: Code = Code::new("MG0104");
pub const UNTERMINATED_STRING: Code = Code::new("MG0105");
pub const UNRECOGNIZED_CHARACTER: Code = Code::new("MG0106");
pub const HEX_INTEGER_OUT_OF_RANGE: Code = Code::new("MG0107");
pub const UNTERMINATED_CHAR: Code = Code::new("MG0108");
pub const EMPTY_CHAR_LITERAL: Code = Code::new("MG0109");
pub const CHAR_LITERAL_MULTIPLE_SCALARS: Code = Code::new("MG0110");
