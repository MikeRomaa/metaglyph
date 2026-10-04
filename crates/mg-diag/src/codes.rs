/// A stable, documented, greppable diagnostic code (spec §13).
///
/// Codes are grouped by class: `MG01xx` syntax, `MG02xx` name resolution,
/// `MG03xx` type checking, `MG04xx` field validation, `MG05xx` path
/// structure, `MG06xx` evaluation and domain errors, `MG07xx` geometry,
/// `MG08xx` export. Each milestone adds its codes here; a shipped code is
/// never renumbered or reused for a different error.
///
/// All codes live in this one file, regardless of which crate raises
/// them, so the whole numbering scheme can be read (and grepped) in one
/// place instead of fragmented across the crates that happen to use each
/// class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code(&'static str);

impl Code {
    pub const fn new(code: &'static str) -> Self {
        Code(code)
    }

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

// -- MG01xx: syntax (mg-syntax: lexer and parser) ------------------------

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

// -- MG02xx: name resolution (mg-hir) -------------------------------------

pub const UNRESOLVED_NAME: Code = Code::new("MG0201");
pub const DUPLICATE_DEFINITION: Code = Code::new("MG0202");
pub const SHADOWS_TOP_LEVEL: Code = Code::new("MG0203");
pub const RESERVED_WORD_NAME: Code = Code::new("MG0204");
pub const RESERVED_ANCHOR_NAME: Code = Code::new("MG0205");
pub const CALL_TO_NON_FUNCTION: Code = Code::new("MG0206");

// -- MG03xx: type checking (mg-hir) ---------------------------------------

pub const TYPE_MISMATCH: Code = Code::new("MG0301");
pub const NO_SUCH_MEMBER: Code = Code::new("MG0302");
pub const MIXED_TUPLE: Code = Code::new("MG0303");
pub const NON_INTEGRAL_INT_FIELD: Code = Code::new("MG0304");
pub const WRONG_ARGUMENT_COUNT: Code = Code::new("MG0305");

// -- MG04xx: field validation (mg-hir) ------------------------------------
//
// Metrics-class errors (a missing reserved metric, `baseline.y != 0`,
// negative `overshoot`) are field-validation errors about a `metric`
// block's configuration, so they live here alongside every other
// field-validation code rather than getting a bucket of their own.

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
/// `font.version` not of the form `digits.digits` (spec §5.6).
pub const MALFORMED_VERSION: Code = Code::new("MG0422");
/// A `variation` selector outside VS1–VS256 (spec §5.6).
pub const INVALID_VARIATION_SELECTOR: Code = Code::new("MG0423");

// -- MG05xx: path structure (mg-hir) --------------------------------------

pub const PATH_NEEDS_BODY: Code = Code::new("MG0501");
pub const PATH_MISSING_START: Code = Code::new("MG0502");
pub const CLOSE_NOT_LAST: Code = Code::new("MG0503");
pub const MULTIPLE_CLOSE: Code = Code::new("MG0504");
pub const MULTIPLE_START: Code = Code::new("MG0505");
pub const START_NOT_FIRST: Code = Code::new("MG0506");
pub const FILL_REQUIRES_CLOSED_PATH: Code = Code::new("MG0507");
/// A `cube` without `c1`, or a `quad` without `c`, whose previous
/// declaration is not a segment of the same kind (spec §6.3 reflection).
pub const INVALID_REFLECTION: Code = Code::new("MG0508");

// -- MG06xx: evaluation and domain errors (mg-eval) -----------------------
//
// Includes cycles, arithmetic/geometric domain errors, and the geometry
// kernel's own errors (spec §7–§8): degenerate stroke/fill input, the
// curvature limit, and self-intersecting fills. MG0610 and MG0612 are
// deliberately unused — they briefly named M3-scope stubs ("stroking not
// yet implemented," "needs Bézier clipping") that M4 fully resolved, so
// per this file's own rule the numbers are retired rather than reused.

pub const CYCLE: Code = Code::new("MG0601");
pub const DIVISION_BY_ZERO: Code = Code::new("MG0602");
pub const SQRT_OF_NEGATIVE: Code = Code::new("MG0603");
pub const INVERSE_TRIG_OUT_OF_RANGE: Code = Code::new("MG0604");
pub const MEET_ON_PARALLEL_LINES: Code = Code::new("MG0605");
pub const PATH_PARAMETER_OUT_OF_DOMAIN: Code = Code::new("MG0606");
pub const EMPTY_LIST_REDUCTION: Code = Code::new("MG0607");
pub const GLYPH_HAS_NO_INK: Code = Code::new("MG0608");
pub const NO_AXIS_ALIGNED_ELLIPSE: Code = Code::new("MG0609");
pub const ZERO_LENGTH_SEGMENT: Code = Code::new("MG0611");
pub const POWER_DOMAIN_ERROR: Code = Code::new("MG0613");
pub const ZERO_VECTOR: Code = Code::new("MG0614");
/// A radii-mode `arc` (spec §6.3) whose chord is longer than `rx`/`ry` can
/// span, or whose `rx`/`ry` is non-positive.
pub const RADII_TOO_SMALL_FOR_CHORD: Code = Code::new("MG0615");
/// A path being stroked or filled has zero total arc length (spec §7.3).
pub const ZERO_LENGTH_PATH: Code = Code::new("MG0616");
/// `stroke` is not greater than zero (spec §7.3).
pub const NON_POSITIVE_STROKE: Code = Code::new("MG0617");
/// The curvature radius drops below `stroke / 2` in a segment's interior
/// (spec §7.2).
pub const CURVATURE_LIMIT_EXCEEDED: Code = Code::new("MG0618");
/// A filled contour crosses itself (spec §8.3).
pub const SELF_INTERSECTING_FILL: Code = Code::new("MG0619");
/// A corner's inner offsets don't cross within its two adjacent segments
/// (spec §7.4): a sharp turn beside a segment too short for the stroke. A
/// 180° reversal also has no crossing, but is a legitimate shape, not this
/// error.
pub const INNER_CORNER_NO_CROSSING: Code = Code::new("MG0620");
/// `lineThrough` of two equal points (spec §5.9).
pub const LINE_THROUGH_ONE_POINT: Code = Code::new("MG0621");
/// An `ellipse` or `circle` radius ≤ 0 (spec §5.9).
pub const NON_POSITIVE_RADIUS: Code = Code::new("MG0622");
/// `cast` with no crossing ahead of the line's origin (spec §5.9).
pub const CAST_MISSES_ELLIPSE: Code = Code::new("MG0623");

// -- MG08xx: export (mg-font) ---------------------------------------------

/// A filled contour that was simple before quantization crosses itself
/// after it (spec §10.4 step 4).
pub const QUANTIZED_FILL_SELF_INTERSECTS: Code = Code::new("MG0801");
/// A point, component offset, or glyph bounding box outside `glyf`'s
/// int16 range (spec §10.5).
pub const COORDINATE_OUT_OF_RANGE: Code = Code::new("MG0802");
/// A glyph with more points or contours than `maxp`'s uint16 counts
/// allow, or a font with more than 65535 glyphs (spec §10.5).
pub const GLYPH_LIMIT_EXCEEDED: Code = Code::new("MG0803");
/// Components nest deeper than `COMPONENT_DEPTH` (spec §10.1).
pub const COMPONENT_TOO_DEEP: Code = Code::new("MG0804");
/// One codepoint mapped by more than one glyph (spec §10.6).
pub const DUPLICATE_CODEPOINT: Code = Code::new("MG0805");
/// A surrogate or noncharacter codepoint (spec §10.6).
pub const UNENCODABLE_CODEPOINT: Code = Code::new("MG0806");
/// An advance width outside `hmtx`'s uint16 range.
pub const ADVANCE_OUT_OF_RANGE: Code = Code::new("MG0807");
/// A `kern` value outside GPOS's int16 range.
pub const KERN_OUT_OF_RANGE: Code = Code::new("MG0808");
/// One variation sequence mapped by more than one glyph (spec §10.6).
pub const DUPLICATE_VARIATION_SEQUENCE: Code = Code::new("MG0809");
/// Warning: a VS1–VS16 sequence Unicode does not standardize (spec §10.6).
pub const UNSTANDARDIZED_VARIATION_SEQUENCE: Code = Code::new("MG0810");
/// Warning: a variation sequence whose base no glyph encodes (spec §10.6).
pub const VARIATION_BASE_NOT_ENCODED: Code = Code::new("MG0811");
