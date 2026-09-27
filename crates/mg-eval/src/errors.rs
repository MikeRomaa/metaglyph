//! Evaluation-time errors (spec §13 Domain class, plus this milestone's
//! own scope markers). Span-free by design: [`crate::construct`] and the
//! rest of the evaluator only know *what* went wrong, never *where* —
//! the caller walking the expression tree is the one holding a span, and
//! attaches it when building the [`mg_diag::Diagnostic`].

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
    UnitOfZeroVector,
    PathParameterOutOfDomain {
        param: f64,
        max: f64,
    },
    EmptyListReduction {
        function: &'static str,
    },
    GlyphHasNoInk,
    /// A segment's direction was left free; Hobby's algorithm is
    /// deferred to M4 (spec plan M3).
    FreeDirection,
    /// A rendering path's `.bbox` needs the stroked/filled outline;
    /// stroking is deferred to M4 (spec plan M3).
    StrokingNotYetImplemented,
    ZeroLengthSegment,
    /// `intersect` between two curved segments needs Bézier clipping,
    /// deferred to M4 (spec plan M3).
    NeedsBezierClipping,
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
            EvalError::UnitOfZeroVector => write!(f, "`unit` of a zero-length pair"),
            EvalError::PathParameterOutOfDomain { param, max } => {
                write!(f, "path parameter {param} is outside [0, {max}]")
            }
            EvalError::EmptyListReduction { function } => {
                write!(f, "`{function}` of an empty list")
            }
            EvalError::GlyphHasNoInk => write!(f, "`.bbox` of a glyph with no ink"),
            EvalError::FreeDirection => {
                write!(
                    f,
                    "a free direction needs Hobby's algorithm, not yet implemented (M4)"
                )
            }
            EvalError::StrokingNotYetImplemented => {
                write!(
                    f,
                    "stroking is not yet implemented (M4); `.bbox` on a rendering path needs it"
                )
            }
            EvalError::ZeroLengthSegment => write!(f, "zero-length segment"),
            EvalError::NeedsBezierClipping => {
                write!(
                    f,
                    "intersecting two curved segments needs Bézier clipping, not yet implemented (M4)"
                )
            }
        }
    }
}

impl From<mg_geom::skeleton::SkeletonError> for EvalError {
    fn from(err: mg_geom::skeleton::SkeletonError) -> Self {
        match err {
            mg_geom::skeleton::SkeletonError::FreeDirection => EvalError::FreeDirection,
            mg_geom::skeleton::SkeletonError::ZeroLengthSegment => EvalError::ZeroLengthSegment,
        }
    }
}

impl From<mg_geom::skeleton::NeedsBezierClipping> for EvalError {
    fn from(_: mg_geom::skeleton::NeedsBezierClipping) -> Self {
        EvalError::NeedsBezierClipping
    }
}
