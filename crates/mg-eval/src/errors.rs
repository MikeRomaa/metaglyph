//! Evaluation-time errors (spec §13 Domain and Geometry classes).
//! Span-free by design: [`crate::construct`] and the rest of the
//! evaluator only know *what* went wrong, never *where* — the caller
//! walking the expression tree is the one holding a span, and attaches it
//! when building the [`mg_diag::Diagnostic`]. A curvature violation and an
//! unresolved inner corner are the exceptions carrying their own extra
//! detail (a segment index, spec §7.2/§7.4): those numbers aren't visible
//! from a span alone, unlike the glyph/path/instance the span's own
//! location already names.

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    DivisionByZero,
    SqrtOfNegative(f64),
    LogOfNonPositive(f64),
    InverseTrigOutOfRange {
        function: &'static str,
        value: f64,
    },
    PowerDomainError {
        base: f64,
        exponent: f64,
    },
    MeetOnParallelLines,
    /// `lineThrough` of two equal points: no direction (spec §5.9).
    LineThroughOnePoint,
    /// An `ellipse` or `circle` radius ≤ 0 (spec §5.9).
    NonPositiveRadius(f64),
    /// `cast` found no crossing ahead of the line's origin (spec §5.9).
    CastMissesEllipse,
    UnitOfZeroVector,
    PathParameterOutOfDomain {
        param: f64,
        max: f64,
    },
    EmptyListReduction {
        function: &'static str,
    },
    GlyphHasNoInk,
    /// A centre-mode `arc`'s two endpoints admit no axis-aligned ellipse
    /// about its `center` (spec §6.3, §13 Geometry class).
    NoAxisAlignedEllipse,
    /// A radii-mode `arc`'s chord is longer than `rx`/`ry` can span, or
    /// `rx`/`ry` is non-positive (spec §6.3, §13 Geometry class).
    RadiiTooSmallForChord,
    ZeroLengthSegment,
    /// A path being stroked or filled has zero total arc length — no
    /// drawn segment beyond `start` (spec §7.3).
    ZeroLengthPath,
    /// `stroke` is not greater than zero (spec §7.3).
    NonPositiveStroke,
    /// The curvature radius drops below `stroke / 2` somewhere in a
    /// segment's interior (spec §7.2). `local_t` is that segment's own
    /// `[0, 1]` domain, not the spec §5.9 path-query one.
    CurvatureLimitExceeded {
        segment_index: usize,
        local_t: std::ops::Range<f64>,
    },
    /// A filled contour crosses itself (spec §8.3); each pair is the two
    /// crossing segments' own spec §5.9 path-query parameters.
    SelfIntersectingFill {
        crossings: Vec<(f64, f64)>,
    },
    /// A corner's inner offsets don't cross within its two adjacent
    /// segments (spec §7.4): a sharp turn beside a segment too short for
    /// the stroke. `segment_index` is the drawn segment the corner ends.
    /// A 180° reversal also has no crossing, but is a legitimate shape,
    /// not this error.
    InnerCornerNoCrossing {
        segment_index: usize,
    },
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::SqrtOfNegative(v) => write!(f, "sqrt of a negative number ({v})"),
            EvalError::LogOfNonPositive(v) => write!(f, "log of a non-positive number ({v})"),
            EvalError::InverseTrigOutOfRange { function, value } => {
                write!(f, "`{function}` argument {value} is outside [-1, 1]")
            }
            EvalError::PowerDomainError { base, exponent } => {
                write!(
                    f,
                    "{base}^{exponent} is not defined (negative base, non-integer exponent)"
                )
            }
            EvalError::MeetOnParallelLines => write!(f, "`meet` on parallel lines"),
            EvalError::LineThroughOnePoint => {
                write!(f, "`lineThrough` of two equal points has no direction")
            }
            EvalError::NonPositiveRadius(r) => {
                write!(f, "an ellipse radius must be greater than 0 (got {r})")
            }
            EvalError::CastMissesEllipse => {
                write!(f, "the ray meets the ellipse nowhere ahead of its origin")
            }
            EvalError::UnitOfZeroVector => write!(f, "`unit` of a zero-length pair"),
            EvalError::PathParameterOutOfDomain { param, max } => {
                write!(f, "path parameter {param} is outside [0, {max}]")
            }
            EvalError::EmptyListReduction { function } => {
                write!(f, "`{function}` of an empty list")
            }
            EvalError::GlyphHasNoInk => write!(f, "`.bbox` of a glyph with no ink"),
            EvalError::NoAxisAlignedEllipse => {
                write!(
                    f,
                    "no axis-aligned ellipse about `center` passes through both endpoints"
                )
            }
            EvalError::RadiiTooSmallForChord => {
                write!(
                    f,
                    "no ellipse with these `rx`/`ry` radii passes through both endpoints"
                )
            }
            EvalError::ZeroLengthSegment => write!(f, "zero-length segment"),
            EvalError::ZeroLengthPath => write!(f, "path has zero total arc length"),
            EvalError::NonPositiveStroke => write!(f, "`stroke` must be greater than 0"),
            EvalError::CurvatureLimitExceeded {
                segment_index,
                local_t,
            } => {
                write!(
                    f,
                    "curvature radius drops below `stroke / 2` in segment {segment_index} \
                     over parameters {:.4}..{:.4}",
                    local_t.start, local_t.end
                )
            }
            EvalError::SelfIntersectingFill { crossings } => {
                write!(f, "self-intersecting filled contour, crossing itself at ")?;
                for (i, (t_a, t_b)) in crossings.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "({t_a:.4}, {t_b:.4})")?;
                }
                Ok(())
            }
            EvalError::InnerCornerNoCrossing { segment_index } => {
                write!(
                    f,
                    "the inner offsets on either side of segment {segment_index}'s corner \
                     don't cross within it — the turn is sharp and the adjacent segment is \
                     too short for the stroke"
                )
            }
        }
    }
}

impl From<mg_geom::skeleton::SkeletonError> for EvalError {
    fn from(err: mg_geom::skeleton::SkeletonError) -> Self {
        match err {
            mg_geom::skeleton::SkeletonError::ZeroLengthSegment => EvalError::ZeroLengthSegment,
            mg_geom::skeleton::SkeletonError::NoAxisAlignedEllipse => {
                EvalError::NoAxisAlignedEllipse
            }
            mg_geom::skeleton::SkeletonError::RadiiTooSmallForChord => {
                EvalError::RadiiTooSmallForChord
            }
        }
    }
}
