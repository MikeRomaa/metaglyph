//! The static type system (spec §5.5, §5.8–§5.10): the `Type` lattice used
//! by [`crate::type_check`], the spec §5.9 function table, member-access
//! tables, and built-in constants. `int` is not a distinct member of
//! `Type` — it is a `Num` plus a per-field integrality constraint, checked
//! separately once a value is known to be constant (spec §5.3).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Num,
    Bool,
    String,
    /// `pair` / `point` (spec §5.5: `point` is an alias of `pair`).
    Pair,
    Line,
    Transform,
    Path,
    Rect,
    Zone,
    /// Only ever a field's static shape (`param range:`), never a
    /// first-class expression type.
    Range,
    List(Box<Type>),
    /// Error-recovery sentinel: propagates silently so one bad subtree
    /// does not cascade into a second diagnostic about its parent.
    Error,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Num => write!(f, "num"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Pair => write!(f, "pair"),
            Type::Line => write!(f, "line"),
            Type::Transform => write!(f, "transform"),
            Type::Path => write!(f, "path"),
            Type::Rect => write!(f, "rect"),
            Type::Zone => write!(f, "zone"),
            Type::Range => write!(f, "range"),
            Type::List(elem) => write!(f, "list<{elem}>"),
            Type::Error => write!(f, "<error>"),
        }
    }
}

impl Type {
    pub fn list_of(elem: Type) -> Type {
        Type::List(Box::new(elem))
    }
}

/// One candidate signature for a spec §5.9 function. A name may have more
/// than one (`scale(s)` vs `scale(sx, sy)`).
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

/// Every callable name (spec §5.9: "Complete. Nothing outside this list is
/// callable."). `None` means the name is not a function at all, which
/// call-position resolution (spec §5.11 rule 1) reports as unresolved.
pub fn lookup_function(name: &str) -> Option<Vec<FnSig>> {
    use Type::*;
    fn sig(params: Vec<Type>, ret: Type) -> FnSig {
        FnSig { params, ret }
    }

    Some(match name {
        "abs" | "sign" | "floor" | "ceil" | "round" | "sqrt" | "exp" | "log" | "sin" | "cos"
        | "tan" | "asin" | "acos" => vec![sig(vec![Num], Num)],
        "atan2" | "min" | "max" => vec![sig(vec![Num, Num], Num)],
        "clamp" | "lerp" => vec![sig(vec![Num, Num, Num], Num)],
        "length" | "angle" => vec![sig(vec![Pair], Num)],
        "unit" | "perpendicular" => vec![sig(vec![Pair], Pair)],
        "dir" => vec![sig(vec![Num], Pair)],
        "dot" | "cross" => vec![sig(vec![Pair, Pair], Num)],
        "meet" => vec![sig(vec![Line, Line], Pair)],
        "mediate" => vec![sig(vec![Pair, Pair, Num], Pair)],
        "project" | "mirror" => vec![sig(vec![Pair, Line], Pair)],
        "polar" => vec![sig(vec![Pair, Num, Num], Pair)],
        "lineThrough" => vec![sig(vec![Pair, Pair], Line)],
        "lineAt" => vec![sig(vec![Pair, Num], Line)],
        "hline" | "vline" => vec![sig(vec![Num], Line)],
        "translate" => vec![sig(vec![Num, Num], Transform)],
        "rotate" | "slant" => vec![sig(vec![Num], Transform)],
        "scale" => vec![sig(vec![Num], Transform), sig(vec![Num, Num], Transform)],
        "reflect" => vec![sig(vec![Line], Transform)],
        "apply" => vec![sig(vec![Transform, Pair], Pair)],
        "pointAt" | "directionAt" | "curvatureAt" | "pointAtLength" => {
            let ret = if name == "curvatureAt" { Num } else { Pair };
            vec![sig(vec![Path, Num], ret)]
        }
        "arcLength" => vec![sig(vec![Path], Num)],
        "intersect" => vec![sig(vec![Path, Path], Type::list_of(Num))],
        "subpath" => vec![sig(vec![Path, Num, Num], Path)],
        "reverse" => vec![sig(vec![Path], Path)],
        "extrema" => vec![sig(vec![Path], Type::list_of(Num))],
        "sum" | "minOf" | "maxOf" => vec![sig(vec![Type::list_of(Num)], Num)],
        _ => return None,
    })
}

/// Every name callable per spec §5.9, for near-miss suggestions when an
/// unresolved call-position identifier is close to a real function name.
pub const FUNCTION_NAMES: &[&str] = &[
    "abs",
    "sign",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "exp",
    "log",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan2",
    "min",
    "max",
    "clamp",
    "lerp",
    "length",
    "angle",
    "unit",
    "dir",
    "dot",
    "cross",
    "perpendicular",
    "meet",
    "mediate",
    "project",
    "polar",
    "mirror",
    "lineThrough",
    "lineAt",
    "hline",
    "vline",
    "translate",
    "rotate",
    "scale",
    "slant",
    "reflect",
    "apply",
    "pointAt",
    "directionAt",
    "curvatureAt",
    "arcLength",
    "pointAtLength",
    "intersect",
    "subpath",
    "reverse",
    "extrema",
    "sum",
    "minOf",
    "maxOf",
];

/// Member access on a value of a known type (spec §5.5), outside the
/// namespace-root special forms (`font.*`, `glyph.*`, `glyphs.<name>.*`,
/// `instance.*`, `math.*`), which [`crate::type_check`] handles directly.
pub fn member_type(receiver: &Type, member: &str) -> Option<Type> {
    match receiver {
        Type::Pair => matches!(member, "x" | "y").then_some(Type::Num),
        Type::Rect => match member {
            "x0" | "y0" | "x1" | "y1" | "width" | "height" => Some(Type::Num),
            "center" => Some(Type::Pair),
            _ => None,
        },
        Type::Zone => matches!(member, "y" | "ink" | "overshoot").then_some(Type::Num),
        Type::Path => (member == "bbox").then_some(Type::Rect),
        _ => None,
    }
}

/// Every legal member name on `receiver`'s type, for "no such member"
/// diagnostics and near-miss suggestions.
pub fn member_names(receiver: &Type) -> &'static [&'static str] {
    match receiver {
        Type::Pair => &["x", "y"],
        Type::Rect => &["x0", "y0", "x1", "y1", "width", "height", "center"],
        Type::Zone => &["y", "ink", "overshoot"],
        Type::Path => &["bbox"],
        _ => &[],
    }
}

/// Built-in constants (spec §5.10), outside `math.*`.
pub fn builtin_constant(name: &str) -> Option<Type> {
    matches!(name, "up" | "down" | "left" | "right" | "identity").then(|| {
        if name == "identity" {
            Type::Transform
        } else {
            Type::Pair
        }
    })
}

/// `math.*` (spec §5.10): `math.pi`, `math.tau`, `math.e`.
pub fn math_member(member: &str) -> Option<Type> {
    matches!(member, "pi" | "tau" | "e").then_some(Type::Num)
}

/// `font.*` (spec §5.10): the `font` directive's own attributes, never
/// params or metrics.
pub fn font_member(member: &str) -> Option<Type> {
    match member {
        "name" | "version" | "designer" | "foundry" | "license" => Some(Type::String),
        "em" => Some(Type::Num),
        _ => None,
    }
}

/// `instance.*` (spec §5.10).
pub fn instance_member(member: &str) -> Option<Type> {
    match member {
        "name" => Some(Type::String),
        "slant" => Some(Type::Num),
        _ => None,
    }
}

/// `glyph.*` (spec §5.10): the current glyph, valid only inside a glyph
/// body. Callers gate availability on context; this only supplies types.
pub fn glyph_member(member: &str) -> Option<Type> {
    match member {
        "name" => Some(Type::String),
        "codepoints" => Some(Type::list_of(Type::Num)),
        "advance" => Some(Type::Num),
        "bbox" => Some(Type::Rect),
        _ => None,
    }
}

/// Hover and completion text for a spec §5.9 function: a placeholder
/// name per parameter of its first signature, and one line of meaning.
/// Types come from [`lookup_function`], not from here.
pub struct FnDoc {
    pub params: &'static [&'static str],
    pub doc: &'static str,
}

/// The [`FnDoc`] of every name in [`FUNCTION_NAMES`] (spec §5.9).
pub fn function_doc(name: &str) -> Option<FnDoc> {
    let (params, doc): (&'static [&'static str], &'static str) = match name {
        "abs" => (&["x"], "Absolute value."),
        "sign" => (&["x"], "−1, 0, or 1."),
        "floor" => (&["x"], "Rounds down."),
        "ceil" => (&["x"], "Rounds up."),
        "round" => (&["x"], "Rounds half away from zero."),
        "sqrt" => (&["x"], "Square root; a domain error below 0."),
        "exp" => (&["x"], "e to the power x."),
        "log" => (&["x"], "Natural logarithm."),
        "sin" => (&["θ"], "Sine; radians."),
        "cos" => (&["θ"], "Cosine; radians."),
        "tan" => (&["θ"], "Tangent; radians."),
        "asin" => (&["x"], "Arcsine, in radians; x in [−1, 1]."),
        "acos" => (&["x"], "Arccosine, in radians; x in [−1, 1]."),
        "atan2" => (&["y", "x"], "The angle of (x, y), in radians."),
        "min" => (&["a", "b"], "The smaller of two numbers."),
        "max" => (&["a", "b"], "The larger of two numbers."),
        "clamp" => (&["x", "lo", "hi"], "x limited to [lo, hi]."),
        "lerp" => (&["a", "b", "t"], "a + (b − a)·t."),
        "length" => (&["v"], "A pair's length."),
        "angle" => (&["v"], "A pair's direction, in radians."),
        "unit" => (&["v"], "A pair scaled to length 1; a domain error at zero."),
        "dir" => (&["θ"], "The unit pair at angle θ."),
        "dot" => (&["a", "b"], "Dot product."),
        "cross" => (&["a", "b"], "The z-component of the cross product."),
        "perpendicular" => (&["v"], "The pair rotated +90°."),
        "meet" => (
            &["a", "b"],
            "Where two lines cross; a domain error when parallel.",
        ),
        "mediate" => (&["a", "b", "t"], "The point a + (b − a)·t."),
        "project" => (&["p", "l"], "The foot of the perpendicular from p to l."),
        "polar" => (&["p", "len", "θ"], "The point len from p at angle θ."),
        "mirror" => (&["p", "l"], "p reflected across l."),
        "lineThrough" => (&["a", "b"], "The line through two points."),
        "lineAt" => (&["p", "θ"], "The line through p at angle θ."),
        "hline" => (&["y"], "The horizontal line at y."),
        "vline" => (&["x"], "The vertical line at x."),
        "translate" => (&["dx", "dy"], "A translation."),
        "rotate" => (&["θ"], "A counter-clockwise rotation about the origin."),
        "scale" => (&["s"], "A uniform scale; `scale(sx, sy)` scales each axis."),
        "slant" => (&["θ"], "A shear: (x, y) → (x + y·tan θ, y)."),
        "reflect" => (&["l"], "A reflection across a line."),
        "apply" => (&["t", "p"], "A transform applied to a point."),
        "pointAt" => (&["path", "t"], "The point at path parameter t in [0, n]."),
        "directionAt" => (&["path", "t"], "The unit tangent at path parameter t."),
        "curvatureAt" => (
            &["path", "t"],
            "The signed curvature at t, positive turning counter-clockwise.",
        ),
        "arcLength" => (&["path"], "The skeleton's total length."),
        "pointAtLength" => (&["path", "s"], "The point at distance s along the path."),
        "intersect" => (
            &["a", "b"],
            "Parameters on a where it crosses b, ascending; empty when none.",
        ),
        "subpath" => (&["path", "t0", "t1"], "The part between two parameters."),
        "reverse" => (&["path"], "The path traversed backwards."),
        "extrema" => (&["path"], "Parameters where x′ = 0 or y′ = 0, ascending."),
        "sum" => (&["xs"], "The sum of a list; 0 when empty."),
        "minOf" => (&["xs"], "The smallest element; a domain error when empty."),
        "maxOf" => (&["xs"], "The largest element; a domain error when empty."),
        _ => return None,
    };
    Some(FnDoc { params, doc })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_has_docs_matching_its_first_signature() {
        for name in FUNCTION_NAMES {
            let doc = function_doc(name).unwrap_or_else(|| panic!("`{name}` has no docs"));
            let sigs = lookup_function(name).unwrap();
            assert_eq!(doc.params.len(), sigs[0].params.len(), "`{name}`");
        }
    }
}
