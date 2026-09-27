//! Segment realization (spec §6.3): turns a path's already-evaluated
//! `start`/`line`/`spline` fields into a skeleton [`BezPath`]. Pure
//! geometry — every point, direction, and tension has already been
//! evaluated by `mg-eval`; this module only applies the direction rules
//! and lays down curves.
//!
//! **Scope for M3:** the four *local* direction rules are implemented in
//! full — an inherited `dir` makes a smooth joint, `fromDir` overrides it
//! and makes a corner, a spline after a line inherits nothing, and
//! `controls` fixes the endpoint tangent directly. What is deferred to M4
//! is Hobby's algorithm for a *free* direction (spec §6.3, §15.2's
//! differential test against `mf`/`mpost`) — [`realize`] reports
//! [`SkeletonError::FreeDirection`] instead of solving for one. The
//! Appendix A conformance sample gives every direction explicitly, so it
//! never hits this.
//!
//! **Known approximation:** placing control points from a *given* pair of
//! endpoint tangent directions still needs a formula, even without
//! Hobby's tridiagonal solve. [`hermite_controls`] uses a plain
//! chord-scaled Hermite construction rather than Hobby's exact velocity
//! function (Hobby 1986). It is geometrically reasonable — the tangents
//! are right, the curve is smooth — but M4 must replace it before the
//! spec §15.2 differential test against `mf`/`mpost` can pass; M3 only
//! needs *a* deterministic curve to exercise the dependency graph and
//! `.bbox` end to end.

use kurbo::{BezPath, PathSeg, Point, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonError {
    /// A segment's departure or arrival direction was left free (no
    /// `dir`, `fromDir`, or `controls` to pin it down), which needs
    /// Hobby's algorithm — not yet implemented (deferred to M4).
    FreeDirection,
    /// `start.at` and a segment's `to` (or two consecutive segments'
    /// endpoints) coincide.
    ZeroLengthSegment,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentKind {
    Line,
    Spline,
}

/// One segment's already-evaluated fields (spec §5.7). `dir` and
/// `from_dir` carry direction only — callers may pass any nonzero
/// vector, not necessarily a unit one.
#[derive(Debug, Clone, Copy)]
pub struct RawSegment {
    pub kind: SegmentKind,
    pub to: Point,
    pub dir: Option<Vec2>,
    pub from_dir: Option<Vec2>,
    /// `(departure, arrival)`, each ≥ 0.75 (spec §5.7); defaults to
    /// `(1.0, 1.0)`.
    pub tension: (f64, f64),
    /// Mutually exclusive with `dir`/`from_dir`/`tension` at the HIR
    /// level (spec §5.7); fixes the curve directly when present.
    pub controls: Option<(Point, Point)>,
}

#[derive(Debug, Clone, Copy)]
pub struct RawStart {
    pub at: Point,
    /// Departure direction of the first segment.
    pub dir: Option<Vec2>,
}

/// Realizes a path body's skeleton (spec §6.3). `closed` mirrors the
/// HIR's own `PathDecl::closed` — when true and the last point does not
/// already coincide with `start.at`, this appends the closing spline the
/// spec describes (§5.7: departure from the last declaration's endpoint
/// tangent when it was a spline with `dir`/`controls`, arrival from
/// `start.dir`, both otherwise free).
pub fn realize(
    start: &RawStart,
    segments: &[RawSegment],
    closed: bool,
) -> Result<BezPath, SkeletonError> {
    let mut path = BezPath::new();
    path.move_to(start.at);

    let mut current = start.at;
    // The direction a smooth joint inherits into the *next* segment: the
    // resolved arrival tangent of whichever segment just ran, or `None`
    // after a `line` (spec §6.3: "a spline after a line inherits
    // nothing") or after a segment whose own arrival was left free.
    let mut inherited_dir = start.dir;

    for segment in segments {
        match segment.kind {
            SegmentKind::Line => {
                if segment.to == current {
                    return Err(SkeletonError::ZeroLengthSegment);
                }
                path.line_to(segment.to);
                current = segment.to;
                inherited_dir = None;
            }
            SegmentKind::Spline => {
                let (c0, c1) = spline_controls(current, segment, inherited_dir)?;
                path.curve_to(c0, c1, segment.to);
                inherited_dir = resolved_arrival_dir(segment, c1);
                current = segment.to;
            }
        }
    }

    if closed {
        if current != start.at {
            let closing = RawSegment {
                kind: SegmentKind::Spline,
                to: start.at,
                dir: start.dir,
                from_dir: None,
                tension: (1.0, 1.0),
                controls: None,
            };
            let (c0, c1) = spline_controls(current, &closing, inherited_dir)?;
            path.curve_to(c0, c1, start.at);
        }
        path.close_path();
    }

    Ok(path)
}

/// This spline's control points, either taken directly from `controls`
/// or built from resolved departure/arrival tangents.
fn spline_controls(
    from: Point,
    segment: &RawSegment,
    inherited_dir: Option<Vec2>,
) -> Result<(Point, Point), SkeletonError> {
    if let Some(controls) = segment.controls {
        return Ok(controls);
    }
    if segment.to == from {
        return Err(SkeletonError::ZeroLengthSegment);
    }
    let departure = segment
        .from_dir
        .or(inherited_dir)
        .ok_or(SkeletonError::FreeDirection)?;
    let arrival = segment.dir.ok_or(SkeletonError::FreeDirection)?;
    Ok(hermite_controls(
        from,
        segment.to,
        departure,
        arrival,
        segment.tension,
    ))
}

/// This segment's resolved arrival tangent, for the next segment's
/// inheritance (spec §6.3): its own `dir` when given, else the tangent
/// implied by `controls`, else free (`None`).
fn resolved_arrival_dir(segment: &RawSegment, c1: Point) -> Option<Vec2> {
    segment.dir.or_else(|| {
        let tangent = segment.to - c1;
        (tangent != Vec2::ZERO).then_some(tangent)
    })
}

/// Places control points from given endpoint tangent directions and
/// tensions, chord-scaled (see the module's known-approximation note).
fn hermite_controls(
    p0: Point,
    p1: Point,
    departure: Vec2,
    arrival: Vec2,
    tension: (f64, f64),
) -> (Point, Point) {
    let chord = p1 - p0;
    let dist = chord.length();
    let c0 = p0 + departure.normalize() * (dist / (3.0 * tension.0));
    let c1 = p1 - arrival.normalize() * (dist / (3.0 * tension.1));
    (c0, c1)
}

/// The skeleton's tight bounding box (spec §5.5 `path.bbox`, for a
/// construction path). Rendering paths use the stroked/filled outline's
/// bounds instead (spec plan M3; `stroke` itself is M4's).
pub fn bounding_box(path: &BezPath) -> kurbo::Rect {
    use kurbo::Shape;
    path.bounding_box()
}

/// Every segment of `path` as a 0-based `Vec`, matching the spec §5.9
/// path-query parameter domain `[0, n]`: segment `i` spans `[i, i+1]`.
pub fn segments(path: &BezPath) -> Vec<PathSeg> {
    path.segments().collect()
}

/// Total arc length, per segment plus in total (spec §5.9 `arcLength`).
pub fn arc_length(path: &BezPath, accuracy: f64) -> f64 {
    use kurbo::ParamCurveArclen;
    segments(path).iter().map(|s| s.arclen(accuracy)).sum()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedsBezierClipping;

/// `a`'s global parameters (spec §5.9 domain `[0, n]`) where `a` crosses
/// `b`, ascending. Handles every pair where at least one side is a
/// straight `Line` segment, via kurbo's own line–curve intersection —
/// which is every segment the M3 skeleton realizer can produce, since it
/// never emits `Quad`. A genuine curve-against-curve crossing (two
/// `Cubic` segments) has no such shortcut; kurbo has no curve–curve
/// solver, and Bézier clipping is deferred to M4 (spec plan M3: "schedule
/// late... not used by the Appendix A sample"), so that pair reports
/// [`NeedsBezierClipping`] instead of a parameter.
pub fn intersect(a: &BezPath, b: &BezPath) -> Result<Vec<f64>, NeedsBezierClipping> {
    let a_segs = segments(a);
    let b_segs = segments(b);
    let mut hits = Vec::new();

    for (i, seg_a) in a_segs.iter().enumerate() {
        for seg_b in &b_segs {
            match (as_line(seg_a), as_line(seg_b)) {
                (Some(line_a), _) => {
                    for hit in seg_b.intersect_line(line_a) {
                        hits.push(i as f64 + hit.line_t);
                    }
                }
                (None, Some(line_b)) => {
                    for hit in seg_a.intersect_line(line_b) {
                        hits.push(i as f64 + hit.segment_t);
                    }
                }
                (None, None) => return Err(NeedsBezierClipping),
            }
        }
    }

    hits.sort_by(|x, y| x.partial_cmp(y).expect("path parameters are always finite"));
    Ok(hits)
}

fn as_line(seg: &PathSeg) -> Option<kurbo::Line> {
    match seg {
        PathSeg::Line(line) => Some(*line),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn seg_line(to: Point) -> RawSegment {
        RawSegment {
            kind: SegmentKind::Line,
            to,
            dir: None,
            from_dir: None,
            tension: (1.0, 1.0),
            controls: None,
        }
    }

    fn seg_spline(to: Point, dir: Option<Vec2>) -> RawSegment {
        RawSegment {
            kind: SegmentKind::Spline,
            to,
            dir,
            from_dir: None,
            tension: (1.0, 1.0),
            controls: None,
        }
    }

    #[test]
    fn straight_lines_realize_directly() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: None,
        };
        let segs = [
            seg_line(Point::new(10.0, 0.0)),
            seg_line(Point::new(10.0, 10.0)),
        ];
        let path = realize(&start, &segs, false).unwrap();
        assert_eq!(path.segments().count(), 2);
        assert_eq!(path.bounding_box(), kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn spline_with_explicit_directions_realizes() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: Some(Vec2::new(1.0, 0.0)),
        };
        let segs = [seg_spline(
            Point::new(10.0, 10.0),
            Some(Vec2::new(0.0, 1.0)),
        )];
        let path = realize(&start, &segs, false).unwrap();
        assert_eq!(path.segments().count(), 1);
    }

    #[test]
    fn free_direction_is_deferred_to_m4() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: None,
        };
        let segs = [seg_spline(Point::new(10.0, 10.0), None)];
        assert_eq!(
            realize(&start, &segs, false),
            Err(SkeletonError::FreeDirection)
        );
    }

    #[test]
    fn spline_after_line_inherits_nothing() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: Some(Vec2::new(1.0, 0.0)),
        };
        let segs = [
            seg_line(Point::new(10.0, 0.0)),
            seg_spline(Point::new(20.0, 10.0), None),
        ];
        // The line resets the inherited direction, so the spline's
        // departure is free — even though `start.dir` was given.
        assert_eq!(
            realize(&start, &segs, false),
            Err(SkeletonError::FreeDirection)
        );
    }

    #[test]
    fn closed_path_appends_closing_spline() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: Some(Vec2::new(1.0, 0.0)),
        };
        let segs = [
            seg_spline(Point::new(10.0, 10.0), Some(Vec2::new(0.0, 1.0))),
            seg_spline(Point::new(0.0, 20.0), Some(Vec2::new(-1.0, 0.0))),
        ];
        let path = realize(&start, &segs, true).unwrap();
        // Two authored splines plus one appended closing spline.
        assert_eq!(path.segments().count(), 3);
    }

    #[test]
    fn already_closed_path_appends_nothing() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: None,
        };
        let segs = [
            seg_line(Point::new(10.0, 0.0)),
            seg_line(Point::new(0.0, 0.0)),
        ];
        let path = realize(&start, &segs, true).unwrap();
        assert_eq!(path.segments().count(), 2);
    }

    #[test]
    fn zero_length_line_is_an_error() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
            dir: None,
        };
        let segs = [seg_line(Point::new(0.0, 0.0))];
        assert_eq!(
            realize(&start, &segs, false),
            Err(SkeletonError::ZeroLengthSegment)
        );
    }
}
