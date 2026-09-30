//! Segment realization (spec §6.3): turns a path's already-evaluated
//! `start`/`line`/`quad`/`cube`/`arc` fields into a skeleton [`BezPath`].
//! Pure geometry — every point has already been evaluated by `mg-eval`;
//! this module only lays down curves and, for `arc`, does the ellipse
//! math. Nothing here is an approximation: `quad` is degree-elevated
//! exactly, `cube` passes through unchanged, and an `arc`'s piecewise
//! cubic is exact for the definition the spec gives (not a tolerance).
//!
//! There is no more free-direction solving (Hobby's algorithm is gone
//! from the spec entirely — see plan 1-research.md, "Why not Hobby
//! splines"), so unlike M3's first cut at this module, every code path
//! here is exact, not a stand-in for later work.

use kurbo::{BezPath, PathSeg, Point, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkeletonError {
    /// `start.at` and a segment's `to` (or two consecutive segments'
    /// endpoints) coincide.
    ZeroLengthSegment,
    /// A centre-mode `arc`'s two endpoints admit no axis-aligned ellipse
    /// about its `center` (spec §6.3): the radius solve is singular and
    /// the endpoints aren't equidistant from `center` within
    /// `ARC_TOLERANCE`, or it has a unique solution with a non-positive
    /// `rx`/`ry`.
    NoAxisAlignedEllipse,
    /// A radii-mode `arc` (spec §6.3) whose chord is longer than `rx`/`ry`
    /// can span beyond `ARC_TOLERANCE`, or whose `rx`/`ry` is
    /// non-positive.
    RadiiTooSmallForChord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Line,
    Quad,
    Cube,
    Arc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep {
    Ccw,
    Cw,
}

/// An `arc`'s ellipse, fixed one of two ways (spec §6.3). Centre mode
/// gives the centre and solves for `rx`/`ry`; radii mode gives `rx`/`ry`
/// and solves for the centre, picking between the two candidate centres
/// via `large` (SVG's endpoint arc, SVG 1.1 Implementation Notes F.6.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArcGeometry {
    Center(Point),
    Radii { rx: f64, ry: f64, large: bool },
}

/// One segment's already-evaluated fields (spec §5.7). `c`/`c1` are
/// `None` exactly when reflecting the previous segment's adjacent control
/// point (spec §6.3) — `mg-hir`'s structural check already guarantees the
/// previous segment is the same kind whenever that's the case, so
/// [`realize`] trusts it rather than re-checking.
#[derive(Debug, Clone, Copy)]
pub enum RawSegment {
    Line {
        to: Point,
    },
    Quad {
        to: Point,
        c: Option<Point>,
    },
    Cube {
        to: Point,
        c1: Option<Point>,
        c2: Point,
    },
    Arc {
        to: Point,
        geometry: ArcGeometry,
        sweep: Sweep,
    },
}

impl RawSegment {
    pub fn kind(&self) -> SegmentKind {
        match self {
            RawSegment::Line { .. } => SegmentKind::Line,
            RawSegment::Quad { .. } => SegmentKind::Quad,
            RawSegment::Cube { .. } => SegmentKind::Cube,
            RawSegment::Arc { .. } => SegmentKind::Arc,
        }
    }

    pub fn to(&self) -> Point {
        match self {
            RawSegment::Line { to }
            | RawSegment::Quad { to, .. }
            | RawSegment::Cube { to, .. }
            | RawSegment::Arc { to, .. } => *to,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RawStart {
    pub at: Point,
}

/// A realized skeleton, plus how many `BezPath` pieces each *authored*
/// segment expanded into. An `arc` becomes `m` cubic pieces (spec §5.9);
/// every other kind is exactly one. The spec §5.9 path-query parameter
/// domain `[0, n]` counts authored segments, not underlying pieces, so
/// anything indexing by parameter needs this to find the right piece —
/// see [`resolve_param`].
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub path: BezPath,
    pub piece_counts: Vec<usize>,
}

impl Skeleton {
    /// Wraps an already-built `BezPath` with one authored segment per
    /// underlying piece — the right shape for a value with no original
    /// `arc`s to preserve piece counts for, such as `subpath`/`reverse`'s
    /// result (spec §5.9: a construction value that never renders).
    pub fn from_path(path: BezPath) -> Self {
        let piece_counts = vec![1; path.segments().count()];
        Self { path, piece_counts }
    }
}

/// Realizes a path body's skeleton (spec §6.3). `closed` mirrors the
/// HIR's own `PathDecl::closed` — when true and the last point does not
/// already coincide with `start.at`, this appends the closing straight
/// line the spec describes (§5.7).
pub fn realize(
    start: &RawStart,
    segments: &[RawSegment],
    closed: bool,
    arc_tolerance: f64,
) -> Result<Skeleton, SkeletonError> {
    let mut path = BezPath::new();
    path.move_to(start.at);

    let mut current = start.at;
    let mut piece_counts = Vec::with_capacity(segments.len() + 1);
    // Reflection state (spec §6.3): the trailing control point of the
    // most recent `quad`/`cube`, cleared whenever a different kind runs.
    let mut prev_quad_c: Option<Point> = None;
    let mut prev_cube_c2: Option<Point> = None;

    for segment in segments {
        if segment.to() == current {
            return Err(SkeletonError::ZeroLengthSegment);
        }
        match *segment {
            RawSegment::Line { to } => {
                path.line_to(to);
                piece_counts.push(1);
                prev_quad_c = None;
                prev_cube_c2 = None;
            }
            RawSegment::Quad { to, c } => {
                let c = c.unwrap_or_else(|| {
                    let prev = prev_quad_c.expect("mg-hir already validated this reflection");
                    reflect_through(current, prev)
                });
                let (c0, c1) = elevate_quad(current, c, to);
                path.curve_to(c0, c1, to);
                piece_counts.push(1);
                prev_quad_c = Some(c);
                prev_cube_c2 = None;
            }
            RawSegment::Cube { to, c1, c2 } => {
                let c1 = c1.unwrap_or_else(|| {
                    let prev = prev_cube_c2.expect("mg-hir already validated this reflection");
                    reflect_through(current, prev)
                });
                path.curve_to(c1, c2, to);
                piece_counts.push(1);
                prev_cube_c2 = Some(c2);
                prev_quad_c = None;
            }
            RawSegment::Arc {
                to,
                geometry,
                sweep,
            } => {
                let pieces = realize_arc(&mut path, current, to, geometry, sweep, arc_tolerance)?;
                piece_counts.push(pieces);
                prev_quad_c = None;
                prev_cube_c2 = None;
            }
        }
        current = segment.to();
    }

    if closed {
        if current != start.at {
            path.line_to(start.at);
            piece_counts.push(1);
        }
        path.close_path();
    }

    Ok(Skeleton { path, piece_counts })
}

/// The reflection of `point` through `pivot` (spec §6.3: `2·p − c′`),
/// SVG's `S`/`T` construction.
fn reflect_through(pivot: Point, point: Point) -> Point {
    pivot + (pivot - point)
}

/// Exact degree elevation from a quadratic (`p0`, `c`, `p1`) to its cubic
/// equivalent (spec §6.3).
fn elevate_quad(p0: Point, c: Point, p1: Point) -> (Point, Point) {
    let c0 = p0 + (c - p0) * (2.0 / 3.0);
    let c1 = p1 + (c - p1) * (2.0 / 3.0);
    (c0, c1)
}

/// Realizes one `arc` (spec §6.3) as `⌈Δ/90°⌉` cubic pieces, appending
/// them to `path`. Returns the piece count.
fn realize_arc(
    path: &mut BezPath,
    p: Point,
    to: Point,
    geometry: ArcGeometry,
    sweep: Sweep,
    arc_tolerance: f64,
) -> Result<usize, SkeletonError> {
    let (center, rx, ry) = arc_ellipse(p, to, geometry, sweep, arc_tolerance)?;

    let pieces = arc_cubics(p, to, center, rx, ry, sweep);
    let piece_count = pieces.len();
    for (c0, c1, end) in pieces {
        path.curve_to(c0, c1, end);
    }

    Ok(piece_count)
}

/// The ellipse an `arc` from `p` to `to` runs along (spec §6.3), as
/// `(center, rx, ry)`: centre mode solves the radii, radii mode the
/// centre. [`realize`] draws the arc on it; an editor uses it to annotate
/// the arc's centre and radius.
pub fn arc_ellipse(
    p: Point,
    to: Point,
    geometry: ArcGeometry,
    sweep: Sweep,
    arc_tolerance: f64,
) -> Result<(Point, f64, f64), SkeletonError> {
    match geometry {
        ArcGeometry::Center(center) => {
            let (rx, ry) = fit_ellipse(p, to, center, arc_tolerance)?;
            Ok((center, rx, ry))
        }
        ArcGeometry::Radii { rx, ry, large } => {
            let center = solve_center_from_radii(p, to, rx, ry, large, sweep, arc_tolerance)?;
            Ok((center, rx, ry))
        }
    }
}

/// The `⌈Δ/90°⌉` cubic pieces (spec §6.3) of the arc of the ellipse
/// centred on `center` with radii `rx`, `ry`, from `p` to `to`, travelling
/// in `sweep`'s direction — each piece as `(c0, c1, end)`, ready for
/// `BezPath::curve_to`. Shared by [`realize_arc`] and, for the one-off
/// circular case a round join needs (`rx == ry`, `crate::stroke`), a
/// caller with no `Skeleton` of its own to build.
pub(crate) fn arc_cubics(
    p: Point,
    to: Point,
    center: Point,
    rx: f64,
    ry: f64,
    sweep: Sweep,
) -> Vec<(Point, Point, Point)> {
    let theta_p = eccentric_angle(p, center, rx, ry);
    let theta_to = eccentric_angle(to, center, rx, ry);
    let delta = swept_angle(theta_p, theta_to, sweep);

    let m = ((delta / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let signed_step = match sweep {
        Sweep::Ccw => delta / m as f64,
        Sweep::Cw => -delta / m as f64,
    };

    let mut theta = theta_p;
    let mut point = p;
    let mut pieces = Vec::with_capacity(m);
    for k in 0..m {
        let theta_next = theta + signed_step;
        let end = if k + 1 == m {
            to
        } else {
            ellipse_point(center, rx, ry, theta_next)
        };
        let k_factor = 4.0 / 3.0 * (signed_step / 4.0).tan();
        let c0 = point + ellipse_deriv(rx, ry, theta) * k_factor;
        let c1 = end - ellipse_deriv(rx, ry, theta_next) * k_factor;
        pieces.push((c0, c1, end));
        point = end;
        theta = theta_next;
    }
    pieces
}

fn ellipse_point(center: Point, rx: f64, ry: f64, theta: f64) -> Point {
    center + Vec2::new(rx * theta.cos(), ry * theta.sin())
}

/// The ellipse's derivative with respect to `theta`, in the direction of
/// *increasing* `theta` (i.e. CCW) — always this convention regardless of
/// `sweep`; [`realize_arc`] folds the sweep direction into `signed_step`
/// instead, which correctly flips the sign here through `theta.tan()`
/// being odd.
fn ellipse_deriv(rx: f64, ry: f64, theta: f64) -> Vec2 {
    Vec2::new(-rx * theta.sin(), ry * theta.cos())
}

fn eccentric_angle(point: Point, center: Point, rx: f64, ry: f64) -> f64 {
    let d = point - center;
    (d.y / ry).atan2(d.x / rx)
}

/// The swept angle from `theta_p` to `theta_to`, travelling in `sweep`'s
/// direction, in `(0, τ)` (spec §6.3: "covering strictly between 0° and
/// 360°"). `theta_p == theta_to` can't arise here: [`realize`] already
/// rejects `p == to` before calling this, and two distinct points on one
/// ellipse never share an eccentric angle.
fn swept_angle(theta_p: f64, theta_to: f64, sweep: Sweep) -> f64 {
    let tau = std::f64::consts::TAU;
    match sweep {
        Sweep::Ccw => (theta_to - theta_p).rem_euclid(tau),
        Sweep::Cw => (theta_p - theta_to).rem_euclid(tau),
    }
}

/// Solves for `(rx, ry)` of the axis-aligned ellipse about `center`
/// passing through `p` and `to` (spec §6.3).
fn fit_ellipse(
    p: Point,
    to: Point,
    center: Point,
    arc_tolerance: f64,
) -> Result<(f64, f64), SkeletonError> {
    let d0 = p - center;
    let d1 = to - center;
    let (dx0_2, dy0_2) = (d0.x * d0.x, d0.y * d0.y);
    let (dx1_2, dy1_2) = (d1.x * d1.x, d1.y * d1.y);
    let len0_sq = dx0_2 + dy0_2;
    let len1_sq = dx1_2 + dy1_2;

    let det = dx0_2 * dy1_2 - dx1_2 * dy0_2;
    let singular_threshold = 1e-12 * len0_sq * len1_sq;

    if det.abs() > singular_threshold {
        let u = (dy1_2 - dy0_2) / det;
        let v = (dx0_2 - dx1_2) / det;
        if u > 0.0 && v > 0.0 {
            Ok((1.0 / u.sqrt(), 1.0 / v.sqrt()))
        } else {
            Err(SkeletonError::NoAxisAlignedEllipse)
        }
    } else {
        let len0 = len0_sq.sqrt();
        let len1 = len1_sq.sqrt();
        if (len0 - len1).abs() <= arc_tolerance && len0 > 0.0 {
            let r = (len0 + len1) / 2.0;
            Ok((r, r))
        } else {
            Err(SkeletonError::NoAxisAlignedEllipse)
        }
    }
}

/// Solves for the centre of a radii-mode `arc` (spec §6.3): SVG's
/// endpoint-to-centre parametrization (SVG 1.1 Implementation Notes
/// F.6.5) with zero rotation. There are two candidate centres, symmetric
/// about the chord `p`–`to`; travelling in `sweep`'s direction, one's arc
/// spans less than 180° and the other's more, and `large` picks between
/// them.
fn solve_center_from_radii(
    p: Point,
    to: Point,
    rx: f64,
    ry: f64,
    large: bool,
    sweep: Sweep,
    arc_tolerance: f64,
) -> Result<Point, SkeletonError> {
    if rx <= 0.0 || ry <= 0.0 {
        return Err(SkeletonError::RadiiTooSmallForChord);
    }

    let mid = Point::new((p.x + to.x) / 2.0, (p.y + to.y) / 2.0);
    let half_chord = Vec2::new((p.x - to.x) / 2.0, (p.y - to.y) / 2.0);
    let scaled = Vec2::new(half_chord.x / rx, half_chord.y / ry);
    let mut lambda = scaled.x * scaled.x + scaled.y * scaled.y;

    if lambda > 1.0 {
        if (lambda.sqrt() - 1.0) * rx.max(ry) <= arc_tolerance {
            // The chord is (within tolerance) a diameter: unlike SVG, the
            // radii are never enlarged to fit, so this is the closest
            // legal ellipse rather than an approximation of a bigger one.
            lambda = 1.0;
        } else {
            return Err(SkeletonError::RadiiTooSmallForChord);
        }
    }

    // `lambda == 1.0` here only when set above or when the chord already
    // is an exact diameter — either way the candidates coincide at `mid`
    // and `large` has no effect (spec §6.3).
    let factor = ((1.0 - lambda) / lambda).sqrt();
    if factor == 0.0 {
        return Ok(mid);
    }

    let offset = Vec2::new(-rx * scaled.y, ry * scaled.x) * factor;
    let candidate_a = mid + offset;
    let candidate_b = mid - offset;

    let span = |center: Point| {
        swept_angle(
            eccentric_angle(p, center, rx, ry),
            eccentric_angle(to, center, rx, ry),
            sweep,
        )
    };
    let (small, big) = if span(candidate_a) <= span(candidate_b) {
        (candidate_a, candidate_b)
    } else {
        (candidate_b, candidate_a)
    };
    Ok(if large { big } else { small })
}

/// The skeleton's tight bounding box (spec §5.5 `path.bbox`, for a
/// construction path). A rendering path's `.bbox` uses the stroked/filled
/// outline's bounds instead (`crate::stroke`, `crate::fill`), not this.
pub fn bounding_box(path: &BezPath) -> kurbo::Rect {
    use kurbo::Shape;
    path.bounding_box()
}

/// Every underlying `BezPath` piece, in order. Not 1:1 with authored
/// segments when an `arc` is present — see [`resolve_param`], which is
/// almost always what you actually want instead.
pub fn segments(path: &BezPath) -> Vec<PathSeg> {
    path.segments().collect()
}

/// Total arc length (spec §5.9 `arcLength`).
pub fn arc_length(path: &BezPath, accuracy: f64) -> f64 {
    use kurbo::ParamCurveArclen;
    segments(path).iter().map(|s| s.arclen(accuracy)).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamOutOfDomain;

/// `t`'s 0-based index into the flattened underlying pieces, and its
/// local parameter within that piece, per the spec §5.9 domain over
/// *authored* segments — `[0, n]`, segment `i` spanning `[i, i+1]`, and
/// (spec §6.3) an `arc` realized as `m` pieces dividing its span
/// uniformly, piece `k` spanning `[i + k/m, i + (k+1)/m]`. Exposed
/// directly (not just via [`resolve_param`]) for callers like `subpath`
/// that need to work across a range of pieces rather than look up one.
pub fn authored_param_to_piece(
    skeleton: &Skeleton,
    t: f64,
) -> Result<(usize, f64), ParamOutOfDomain> {
    let n = skeleton.piece_counts.len();
    if n == 0 || t < 0.0 || t > n as f64 {
        return Err(ParamOutOfDomain);
    }
    let seg_index = if t >= n as f64 {
        n - 1
    } else {
        t.floor() as usize
    };
    let within_segment = t - seg_index as f64;

    let m = skeleton.piece_counts[seg_index];
    let piece_offset: usize = skeleton.piece_counts[..seg_index].iter().sum();
    let scaled = within_segment * m as f64;
    let piece_index = if scaled >= m as f64 {
        m - 1
    } else {
        scaled.floor() as usize
    };
    let local_t = scaled - piece_index as f64;

    Ok((piece_offset + piece_index, local_t))
}

/// `t`'s underlying piece and local parameter — see
/// [`authored_param_to_piece`] for the domain this indexes into.
pub fn resolve_param(skeleton: &Skeleton, t: f64) -> Result<(PathSeg, f64), ParamOutOfDomain> {
    let (piece_index, local_t) = authored_param_to_piece(skeleton, t)?;
    let pieces = segments(&skeleton.path);
    Ok((pieces[piece_index], local_t))
}

/// The inverse of [`resolve_param`]'s piece lookup: given a 0-based index
/// into the *flattened* underlying pieces and a local parameter within
/// it, the authored-domain global parameter (spec §5.9).
pub(crate) fn piece_to_authored_param(
    skeleton: &Skeleton,
    piece_index: usize,
    local_t: f64,
) -> f64 {
    let mut offset = 0;
    for (seg_index, &m) in skeleton.piece_counts.iter().enumerate() {
        if piece_index < offset + m {
            let k = piece_index - offset;
            return seg_index as f64 + (k as f64 + local_t) / m as f64;
        }
        offset += m;
    }
    panic!("piece_index out of range for this skeleton")
}

/// Parameters where `x′ = 0` or `y′ = 0` (spec §5.9 `extrema`), in the
/// authored-segment domain.
pub fn extrema(skeleton: &Skeleton) -> Vec<f64> {
    use kurbo::ParamCurveExtrema;
    let mut out: Vec<f64> = segments(&skeleton.path)
        .iter()
        .enumerate()
        .flat_map(|(i, seg)| {
            let local: Vec<f64> = match seg {
                PathSeg::Line(l) => l.extrema().into_iter().collect(),
                PathSeg::Quad(q) => q.extrema().into_iter().collect(),
                PathSeg::Cubic(c) => c.extrema().into_iter().collect(),
            };
            local
                .into_iter()
                .map(move |t| piece_to_authored_param(skeleton, i, t))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).expect("path parameters are always finite"));
    out
}

/// The tangent direction at `t` (spec §5.9 `directionAt`, unnormalized).
/// `PathSeg` itself has no `ParamCurveDeriv` impl — only its concrete
/// variants do — so this dispatches by hand.
pub fn direction_at(seg: &PathSeg, t: f64) -> Vec2 {
    use kurbo::{ParamCurve, ParamCurveDeriv};
    match seg {
        PathSeg::Line(line) => line.deriv().eval(t).to_vec2(),
        PathSeg::Quad(quad) => quad.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(cubic) => cubic.deriv().eval(t).to_vec2(),
    }
}

/// Signed curvature at `t` (spec §5.9 `curvatureAt`): positive when
/// turning counter-clockwise, matching kurbo's own convention in a
/// right-handed (here, y-up) coordinate system.
pub fn curvature_at(seg: &PathSeg, t: f64) -> f64 {
    use kurbo::ParamCurveCurvature;
    match seg {
        PathSeg::Line(line) => line.curvature(t),
        PathSeg::Quad(quad) => quad.curvature(t),
        PathSeg::Cubic(cubic) => cubic.curvature(t),
    }
}

/// `a`'s global parameters (spec §5.9 domain `[0, n]`, over authored
/// segments — see [`resolve_param`]) where `a` crosses `b`, ascending.
/// Every pair is handled: a line-involving pair via kurbo's own
/// closed-form solver, a cubic-against-cubic pair via
/// `crate::intersect`'s recursive subdivision (spec plan M4).
pub fn intersect(a: &Skeleton, b: &BezPath, tolerance: f64) -> Vec<f64> {
    let a_segs = segments(&a.path);
    let b_segs = segments(b);
    let mut hits = Vec::new();

    for (i, &seg_a) in a_segs.iter().enumerate() {
        for &seg_b in &b_segs {
            for crossing in crate::intersect::segment_intersections(seg_a, seg_b, tolerance) {
                hits.push(piece_to_authored_param(a, i, crossing.t_a));
            }
        }
    }

    hits.sort_by(|x, y| x.partial_cmp(y).expect("path parameters are always finite"));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    const NO_TOLERANCE: f64 = 1e-6;

    fn line(to: Point) -> RawSegment {
        RawSegment::Line { to }
    }

    #[test]
    fn straight_lines_realize_directly() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [line(Point::new(10.0, 0.0)), line(Point::new(10.0, 10.0))];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.path.segments().count(), 2);
        assert_eq!(skeleton.piece_counts, vec![1, 1]);
        assert_eq!(
            skeleton.path.bounding_box(),
            kurbo::Rect::new(0.0, 0.0, 10.0, 10.0)
        );
    }

    #[test]
    fn quad_elevates_to_the_exact_cubic() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [RawSegment::Quad {
            to: Point::new(10.0, 0.0),
            c: Some(Point::new(5.0, 10.0)),
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        let PathSeg::Cubic(cubic) = skeleton.path.segments().next().unwrap() else {
            panic!("expected a cubic")
        };
        // Sample the quadratic and the elevated cubic at the same
        // parameters; a degree-elevated curve is identical everywhere.
        use kurbo::{ParamCurve, QuadBez};
        let quad = QuadBez::new(
            Point::new(0.0, 0.0),
            Point::new(5.0, 10.0),
            Point::new(10.0, 0.0),
        );
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let a = quad.eval(t);
            let b = cubic.eval(t);
            assert!((a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        }
    }

    #[test]
    fn quad_reflection_mirrors_the_previous_control_point() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Quad {
                to: Point::new(10.0, 0.0),
                c: Some(Point::new(5.0, 10.0)),
            },
            RawSegment::Quad {
                to: Point::new(20.0, 0.0),
                c: None,
            },
        ];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.piece_counts, vec![1, 1]);
        // The reflection of (5, 10) through (10, 0) is (15, -10).
        let PathSeg::Cubic(second) = skeleton.path.segments().nth(1).unwrap() else {
            panic!("expected a cubic")
        };
        let (expected_c0, _) = elevate_quad(
            Point::new(10.0, 0.0),
            Point::new(15.0, -10.0),
            Point::new(20.0, 0.0),
        );
        assert!((second.p1.x - expected_c0.x).abs() < 1e-9);
        assert!((second.p1.y - expected_c0.y).abs() < 1e-9);
    }

    #[test]
    fn cube_passes_through_unchanged() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let c1 = Point::new(2.0, 5.0);
        let c2 = Point::new(8.0, 5.0);
        let to = Point::new(10.0, 0.0);
        let segs = [RawSegment::Cube {
            to,
            c1: Some(c1),
            c2,
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        let PathSeg::Cubic(cubic) = skeleton.path.segments().next().unwrap() else {
            panic!("expected a cubic")
        };
        assert_eq!((cubic.p1, cubic.p2, cubic.p3), (c1, c2, to));
    }

    #[test]
    fn quarter_circle_arc_is_one_piece_and_hits_the_exact_endpoint() {
        // Quarter circle of radius 10 about the origin, from (10, 0) to
        // (0, 10), travelling ccw.
        let start = RawStart {
            at: Point::new(10.0, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(0.0, 10.0),
            geometry: ArcGeometry::Center(Point::new(0.0, 0.0)),
            sweep: Sweep::Ccw,
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.piece_counts, vec![1]);
        let PathSeg::Cubic(cubic) = skeleton.path.segments().next().unwrap() else {
            panic!("expected a cubic")
        };
        assert!((cubic.p3.x - 0.0).abs() < 1e-9);
        assert!((cubic.p3.y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn three_quarter_circle_arc_is_three_pieces() {
        let start = RawStart {
            at: Point::new(10.0, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(0.0, -10.0),
            geometry: ArcGeometry::Center(Point::new(0.0, 0.0)),
            sweep: Sweep::Ccw,
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.piece_counts, vec![3]);
        let last = skeleton.path.segments().last().unwrap();
        let PathSeg::Cubic(cubic) = last else {
            panic!("expected a cubic")
        };
        assert!((cubic.p3.x - 0.0).abs() < 1e-9);
        assert!((cubic.p3.y + 10.0).abs() < 1e-9);
    }

    #[test]
    fn arc_stays_within_bound_of_the_exact_ellipse() {
        let center = Point::new(0.0, 0.0);
        let (rx, ry) = (10.0, 6.0);
        let start_pt = ellipse_point(center, rx, ry, 0.0);
        let end_pt = ellipse_point(center, rx, ry, 200f64.to_radians());
        let start = RawStart { at: start_pt };
        let segs = [RawSegment::Arc {
            to: end_pt,
            geometry: ArcGeometry::Center(center),
            sweep: Sweep::Ccw,
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();

        use kurbo::ParamCurve;
        let bound = 3e-4 * rx.max(ry);
        for seg in skeleton.path.segments() {
            let PathSeg::Cubic(cubic) = seg else {
                panic!("expected a cubic")
            };
            for i in 0..=20 {
                let t = i as f64 / 20.0;
                let p = cubic.eval(t);
                let d = p - center;
                // Distance from the sample to the ellipse boundary,
                // approximated via the normalized radial residual scaled
                // by the smaller radius (adequate near-boundary bound for
                // this test's purpose).
                let residual = ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt() - 1.0;
                assert!(
                    residual.abs() * rx.min(ry) < bound,
                    "residual {residual} at t={t}"
                );
            }
        }
    }

    #[test]
    fn singular_system_falls_back_to_circular() {
        // p and to symmetric about the x-axis through center: a circle
        // of radius 5 is a valid (if not unique) fit.
        let center = Point::new(0.0, 0.0);
        let p = Point::new(5.0, 0.0);
        let to = Point::new(-5.0, 0.0);
        let (rx, ry) = fit_ellipse(p, to, center, 1e-6).unwrap();
        assert!((rx - 5.0).abs() < 1e-9);
        assert!((ry - 5.0).abs() < 1e-9);
    }

    #[test]
    fn no_ellipse_fits_is_a_geometry_error() {
        // Singular system (both on the x-axis) but not equidistant from
        // center: no axis-aligned ellipse threads both.
        let center = Point::new(0.0, 0.0);
        let p = Point::new(5.0, 0.0);
        let to = Point::new(-8.0, 0.0);
        assert_eq!(
            fit_ellipse(p, to, center, 1e-6),
            Err(SkeletonError::NoAxisAlignedEllipse)
        );
    }

    /// Asserts every sampled point of `skeleton`'s (single-arc) path lies
    /// on the ellipse of radii `rx`, `ry` about `center`, per the same
    /// residual bound as `arc_stays_within_bound_of_the_exact_ellipse`.
    fn assert_arc_centered_at(skeleton: &Skeleton, center: Point, rx: f64, ry: f64) {
        use kurbo::ParamCurve;
        let bound = 3e-4 * rx.max(ry);
        for seg in skeleton.path.segments() {
            let PathSeg::Cubic(cubic) = seg else {
                panic!("expected a cubic")
            };
            for i in 0..=20 {
                let t = i as f64 / 20.0;
                let p = cubic.eval(t);
                let d = p - center;
                let residual = ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt() - 1.0;
                assert!(
                    residual.abs() * rx.min(ry) < bound,
                    "residual {residual} at t={t}, center {center:?}"
                );
            }
        }
    }

    #[test]
    fn radii_mode_large_flag_picks_the_stated_candidate_center() {
        // p=(10,0), to=(0,10), rx=ry=10: the two circles of radius 10
        // through both points are centered at (0,0) (quarter turn) and
        // (10,10) (three-quarter turn) travelling ccw.
        let start = RawStart {
            at: Point::new(10.0, 0.0),
        };
        let to = Point::new(0.0, 10.0);
        let (rx, ry) = (10.0, 10.0);

        let minor = realize(
            &start,
            &[RawSegment::Arc {
                to,
                geometry: ArcGeometry::Radii {
                    rx,
                    ry,
                    large: false,
                },
                sweep: Sweep::Ccw,
            }],
            false,
            NO_TOLERANCE,
        )
        .unwrap();
        assert_eq!(minor.piece_counts, vec![1]);
        assert_arc_centered_at(&minor, Point::new(0.0, 0.0), rx, ry);

        let major = realize(
            &start,
            &[RawSegment::Arc {
                to,
                geometry: ArcGeometry::Radii {
                    rx,
                    ry,
                    large: true,
                },
                sweep: Sweep::Ccw,
            }],
            false,
            NO_TOLERANCE,
        )
        .unwrap();
        assert_eq!(major.piece_counts, vec![3]);
        assert_arc_centered_at(&major, Point::new(10.0, 10.0), rx, ry);
    }

    #[test]
    fn radii_mode_exact_diameter_solves_a_half_oval() {
        // The chord is exactly the ellipse's major axis: both candidate
        // centers coincide at the midpoint, and `large` has no effect.
        let start = RawStart {
            at: Point::new(5.0, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(-5.0, 0.0),
            geometry: ArcGeometry::Radii {
                rx: 5.0,
                ry: 3.0,
                large: false,
            },
            sweep: Sweep::Ccw,
        }];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        // A half-oval spans exactly 180 degrees: ceil(180/90) = 2 pieces.
        assert_eq!(skeleton.piece_counts, vec![2]);
        assert_arc_centered_at(&skeleton, Point::new(0.0, 0.0), 5.0, 3.0);
    }

    #[test]
    fn radii_mode_near_diameter_chord_falls_back_within_tolerance() {
        // The chord is a hair longer than the major axis, but within
        // `arc_tolerance`: taken as an exact diameter rather than an error.
        let tolerance = 0.01;
        let start = RawStart {
            at: Point::new(5.0001, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(-5.0001, 0.0),
            geometry: ArcGeometry::Radii {
                rx: 5.0,
                ry: 5.0,
                large: false,
            },
            sweep: Sweep::Ccw,
        }];
        let skeleton = realize(&start, &segs, false, tolerance).unwrap();
        assert_eq!(skeleton.piece_counts, vec![2]);
    }

    #[test]
    fn radii_mode_chord_too_long_for_radii_is_an_error() {
        let start = RawStart {
            at: Point::new(5.0, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(-5.0, 0.0),
            geometry: ArcGeometry::Radii {
                rx: 2.0,
                ry: 2.0,
                large: false,
            },
            sweep: Sweep::Ccw,
        }];
        assert_eq!(
            realize(&start, &segs, false, NO_TOLERANCE),
            Err(SkeletonError::RadiiTooSmallForChord)
        );
    }

    #[test]
    fn radii_mode_non_positive_radius_is_an_error() {
        let start = RawStart {
            at: Point::new(5.0, 0.0),
        };
        let segs = [RawSegment::Arc {
            to: Point::new(-5.0, 0.0),
            geometry: ArcGeometry::Radii {
                rx: 0.0,
                ry: 5.0,
                large: false,
            },
            sweep: Sweep::Ccw,
        }];
        assert_eq!(
            realize(&start, &segs, false, NO_TOLERANCE),
            Err(SkeletonError::RadiiTooSmallForChord)
        );
    }

    #[test]
    fn zero_length_line_is_an_error() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [line(Point::new(0.0, 0.0))];
        assert_eq!(
            realize(&start, &segs, false, NO_TOLERANCE),
            Err(SkeletonError::ZeroLengthSegment)
        );
    }

    #[test]
    fn closed_path_appends_a_straight_line() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [line(Point::new(10.0, 0.0)), line(Point::new(10.0, 10.0))];
        let skeleton = realize(&start, &segs, true, NO_TOLERANCE).unwrap();
        // Two authored lines plus one appended closing line.
        assert_eq!(skeleton.path.segments().count(), 3);
        assert_eq!(skeleton.piece_counts, vec![1, 1, 1]);
    }

    #[test]
    fn already_closed_path_appends_nothing() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [line(Point::new(10.0, 0.0)), line(Point::new(0.0, 0.0))];
        let skeleton = realize(&start, &segs, true, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.path.segments().count(), 2);
        assert_eq!(skeleton.piece_counts, vec![1, 1]);
    }

    #[test]
    fn resolve_param_maps_through_arc_pieces() {
        let start = RawStart {
            at: Point::new(10.0, 0.0),
        };
        let segs = [
            line(Point::new(20.0, 0.0)),
            RawSegment::Arc {
                to: Point::new(-20.0, 0.0),
                geometry: ArcGeometry::Center(Point::new(0.0, 0.0)),
                sweep: Sweep::Ccw,
            },
        ];
        // Re-center so the arc's center is the origin: adjust the line
        // start to keep it simple.
        let start = RawStart { at: start.at };
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(skeleton.piece_counts.len(), 2);
        let arc_pieces = skeleton.piece_counts[1];
        assert!(arc_pieces >= 2); // a half circle needs at least 2 pieces

        // t=1.0 is exactly the arc's start (global segment index 1).
        let (_, local_t) = resolve_param(&skeleton, 1.0).unwrap();
        assert_eq!(local_t, 0.0);
        // t=2.0 is exactly the arc's end.
        let (last_piece, local_t) = resolve_param(&skeleton, 2.0).unwrap();
        assert_eq!(local_t, 1.0);
        let PathSeg::Cubic(cubic) = last_piece else {
            panic!("expected a cubic")
        };
        assert!((cubic.p3.x + 20.0).abs() < 1e-6);
    }

    #[test]
    fn param_out_of_domain_is_an_error() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [line(Point::new(10.0, 0.0))];
        let skeleton = realize(&start, &segs, false, NO_TOLERANCE).unwrap();
        assert_eq!(resolve_param(&skeleton, 1.5), Err(ParamOutOfDomain));
        assert_eq!(resolve_param(&skeleton, -0.1), Err(ParamOutOfDomain));
    }
}
