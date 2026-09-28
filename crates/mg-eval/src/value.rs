//! Runtime values (spec §5.5), built on kurbo where a kurbo type already
//! matches the spec exactly (`Point` for `pair`/`point`, `Line` for
//! `line`, `Affine` for `transform`, `BezPath` for `path`), plus our own
//! [`Rect`] and [`Zone`] where it doesn't: kurbo's `Rect` has no
//! `.width`/`.height`/`.center`, and nothing in kurbo represents a
//! `metric` binding's `.y`/`.ink`/`.overshoot`.
//!
//! `range` (spec §5.5: "only as `param range:`") never flows through an
//! expression as a first-class value, so it has no `Value` variant —
//! `mg-hir` already reads a `range` field's bounds directly (see
//! `mg_hir::model::ParamDecl::range`).

use kurbo::{Affine, Line, Point};
use mg_geom::skeleton::Skeleton;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> Point {
        Point::new((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    pub fn union_pt(&self, p: Point) -> Rect {
        Rect {
            x0: self.x0.min(p.x),
            y0: self.y0.min(p.y),
            x1: self.x1.max(p.x),
            y1: self.y1.max(p.y),
        }
    }
}

impl From<kurbo::Rect> for Rect {
    fn from(r: kurbo::Rect) -> Self {
        Rect {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

/// A `metric` declaration's value (spec §5.6): `.y` is the flat position,
/// `.ink` is where a round glyph reaches (`y + overshoot` when `align` is
/// `"top"`, `y − overshoot` when `"bottom"`), and `.overshoot` is given
/// verbatim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    pub y: f64,
    pub overshoot: f64,
    pub ink: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    String(String),
    /// `pair` / `point` (spec §5.5: `point` is an alias of `pair`).
    Pair(Point),
    Line(Line),
    Transform(Affine),
    /// A realized skeleton (see `mg_geom::skeleton`), plus the
    /// authored-segment piece counts a parameter-domain query needs (spec
    /// §5.9: an `arc` divides its span across several underlying cubic
    /// pieces). `subpath`/`reverse` results are the same variant, since
    /// spec §5.9 gives them no separate type.
    Path(Skeleton),
    Rect(Rect),
    Zone(Zone),
    List(Vec<Value>),
    /// A `GlyphBbox` node's value for a glyph with no ink. Never an
    /// expression's value: an inkless glyph is legal (a space), and only
    /// reading its `.bbox` is the spec §13 domain error, reported where
    /// the read happens.
    NoInk,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::String(s) => write!(f, "{s:?}"),
            Value::Pair(p) => write!(f, "({}, {})", p.x, p.y),
            Value::Line(l) => write!(f, "line[({}, {})-({}, {})]", l.p0.x, l.p0.y, l.p1.x, l.p1.y),
            Value::Transform(_) => write!(f, "transform"),
            Value::Path(p) => write!(f, "path[{} segments]", p.piece_counts.len()),
            Value::Rect(r) => write!(f, "rect[{}, {}, {}, {}]", r.x0, r.y0, r.x1, r.y1),
            Value::Zone(z) => write!(f, "zone[y={}, ink={}]", z.y, z.ink),
            Value::NoInk => write!(f, "no ink"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "num",
            Value::Bool(_) => "bool",
            Value::String(_) => "string",
            Value::Pair(_) => "pair",
            Value::Line(_) => "line",
            Value::Transform(_) => "transform",
            Value::Path(_) => "path",
            Value::Rect(_) => "rect",
            Value::Zone(_) => "zone",
            Value::List(_) => "list",
            Value::NoInk => "no ink",
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_pair(&self) -> Option<Point> {
        match self {
            Value::Pair(p) => Some(*p),
            _ => None,
        }
    }

    pub fn as_line(&self) -> Option<Line> {
        match self {
            Value::Line(l) => Some(*l),
            _ => None,
        }
    }

    pub fn as_transform(&self) -> Option<Affine> {
        match self {
            Value::Transform(a) => Some(*a),
            _ => None,
        }
    }

    pub fn as_path(&self) -> Option<&Skeleton> {
        match self {
            Value::Path(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_rect(&self) -> Option<Rect> {
        match self {
            Value::Rect(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_zone(&self) -> Option<Zone> {
        match self {
            Value::Zone(z) => Some(*z),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_num_list(&self) -> Option<Vec<f64>> {
        self.as_list()?.iter().map(Value::as_num).collect()
    }
}
