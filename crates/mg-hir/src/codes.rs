//! `MG02xx`–`MG05xx` — HIR diagnostics (spec §13: name resolution, type,
//! field validation, path structure). Grouping follows `mg_diag::Code`'s
//! documented scheme; metrics-class errors (a missing reserved metric,
//! `baseline.y != 0`, negative `overshoot`) are field-validation errors
//! about a `metric` block's configuration, so they live in `MG04xx`
//! alongside every other field-validation code rather than getting a
//! bucket of their own.
use mg_diag::Code;

// -- MG02xx: name resolution --------------------------------------------

pub const UNRESOLVED_NAME: Code = Code::new("MG0201");
pub const DUPLICATE_DEFINITION: Code = Code::new("MG0202");
pub const CASE_FOLD_COLLISION: Code = Code::new("MG0203");
pub const SHADOWS_TOP_LEVEL: Code = Code::new("MG0204");
pub const RESERVED_WORD_NAME: Code = Code::new("MG0205");
pub const RESERVED_ANCHOR_NAME: Code = Code::new("MG0206");
pub const CALL_TO_NON_FUNCTION: Code = Code::new("MG0207");

// -- MG03xx: type checking -----------------------------------------------

pub const TYPE_MISMATCH: Code = Code::new("MG0301");
pub const NO_SUCH_MEMBER: Code = Code::new("MG0302");
pub const MIXED_TUPLE: Code = Code::new("MG0303");
pub const NON_INTEGRAL_INT_FIELD: Code = Code::new("MG0304");
pub const WRONG_ARGUMENT_COUNT: Code = Code::new("MG0305");

// -- MG04xx: field validation ---------------------------------------------

pub const UNKNOWN_FIELD: Code = Code::new("MG0401");
pub const UNKNOWN_ENUM_VALUE: Code = Code::new("MG0402");
pub const MISSING_REQUIRED_FIELD: Code = Code::new("MG0403");
pub const MUTUALLY_EXCLUSIVE_FIELDS: Code = Code::new("MG0404");
pub const FIELD_ILLEGAL_HERE: Code = Code::new("MG0405");
pub const NON_CONSTANT_EXPRESSION: Code = Code::new("MG0406");
pub const VALUE_OUT_OF_RANGE: Code = Code::new("MG0407");
pub const PARAM_NAMED_AFTER_INSTANCE_FIELD: Code = Code::new("MG0408");
pub const ALTERNATE_WITHOUT_DEFAULT: Code = Code::new("MG0409");
pub const ALTERNATE_WITH_CODEPOINT: Code = Code::new("MG0410");
pub const CODEPOINT_OUT_OF_RANGE: Code = Code::new("MG0411");
pub const UNKNOWN_GLYPH_SET: Code = Code::new("MG0412");
pub const MISSING_REQUIRED_METRIC: Code = Code::new("MG0413");
pub const BASELINE_NOT_ZERO: Code = Code::new("MG0414");
pub const NEGATIVE_OVERSHOOT: Code = Code::new("MG0415");
pub const GLYPH_NAME_TOO_LONG: Code = Code::new("MG0416");
pub const EMPTY_GLYPH_LIST: Code = Code::new("MG0417");
pub const KERN_GROUP_OVERLAP: Code = Code::new("MG0418");
pub const DUPLICATE_KERN_PAIR: Code = Code::new("MG0419");
pub const MISSING_DECLARATION_NAME: Code = Code::new("MG0420");
pub const UNEXPECTED_DECLARATION_NAME: Code = Code::new("MG0421");

// -- MG05xx: path structure -----------------------------------------------

pub const PATH_NEEDS_BODY_OR_FOLLOWS: Code = Code::new("MG0501");
pub const FOLLOWS_TARGET_HAS_NO_BODY: Code = Code::new("MG0502");
pub const PATH_MISSING_START: Code = Code::new("MG0503");
pub const CLOSE_NOT_LAST: Code = Code::new("MG0504");
pub const MULTIPLE_CLOSE: Code = Code::new("MG0505");
pub const MULTIPLE_START: Code = Code::new("MG0506");
pub const START_NOT_FIRST: Code = Code::new("MG0507");
pub const FILL_REQUIRES_CLOSED_PATH: Code = Code::new("MG0508");
