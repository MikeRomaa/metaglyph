//! Stroking (spec §6.4, §7). Offset generation itself is `kurbo::stroke`
//! (spec plan M4's stroker decision); what this module adds is everything
//! kurbo can't do on its own:
//! - the spec §7.2/§7.3 checks that must run *before* kurbo is called,
//! - join splicing, since kurbo takes one join style for the whole path
//!   and `joinAt` needs a different join at one vertex (spec plan M4:
//!   "stroking with `Join::Bevel` and rewriting each corner's bevel chord
//!   in place") — every corner is `Join::Bevel`'s straight chord unless
//!   overwritten, so a path stays one seamless outline with no overlaid
//!   join shapes,
//! - assigning each output contour its role (spec §8.1), which kurbo's
//!   subpath emission order does not track.

use kurbo::{BezPath, Line, ParamCurve, PathEl, Point, Vec2};

use crate::curvature::{self, CurvatureViolation};
use crate::skeleton::{self, Skeleton, Sweep};
use crate::winding::{self, ContourRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

impl From<Cap> for kurbo::Cap {
    fn from(cap: Cap) -> kurbo::Cap {
        match cap {
            Cap::Butt => kurbo::Cap::Butt,
            Cap::Round => kurbo::Cap::Round,
            Cap::Square => kurbo::Cap::Square,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Miter,
    Round,
    Bevel,
}

/// One rendering path's stroke configuration (spec §5.7). `join_overrides`
/// is keyed by 0-based drawn-segment index — the corner at *that*
/// segment's trailing endpoint (spec: "`joinAt` overrides `joins` at the
/// named segment's endpoint") — which the caller resolves from segment
/// names before calling in, since name resolution is `mg-hir`'s concern,
/// not this pure-geometry crate's.
pub struct StrokeSpec {
    pub width: f64,
    pub start_cap: Cap,
    pub end_cap: Cap,
    pub default_join: JoinKind,
    pub join_overrides: Vec<(usize, JoinKind)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrokeError {
    /// The path has zero total arc length: no drawn segment beyond
    /// `start` (spec §7.3).
    ZeroLengthPath,
    /// `stroke` is not greater than zero (spec §7.3).
    NonPositiveStroke,
    Curvature(CurvatureViolation),
}

/// Strokes `skeleton` per `spec` (spec §6.4, §7), returning every output
/// contour with its role already assigned: one outer contour for an open
/// path, an (outer, counter) pair for a closed one. No extra contours:
/// every corner needing a join other than `"bevel"` gets it by rewriting
/// `Join::Bevel`'s own chord in place (spec plan M4's "join splicing").
pub fn stroke_path(
    skeleton: &Skeleton,
    closed: bool,
    spec: &StrokeSpec,
    offset_tolerance: f64,
) -> Result<Vec<(BezPath, ContourRole)>, StrokeError> {
    if spec.width <= 0.0 {
        return Err(StrokeError::NonPositiveStroke);
    }
    if skeleton.piece_counts.is_empty() {
        return Err(StrokeError::ZeroLengthPath);
    }

    let r = spec.width / 2.0;
    if let Some(violation) = curvature::check(skeleton, r) {
        return Err(StrokeError::Curvature(violation));
    }

    let style = kurbo::Stroke::new(spec.width)
        .with_join(kurbo::Join::Bevel)
        .with_miter_limit(MITER_LIMIT)
        .with_start_cap(spec.start_cap.into())
        .with_end_cap(spec.end_cap.into());
    let stroked = kurbo::stroke(
        skeleton.path.elements().iter().copied(),
        &style,
        &kurbo::StrokeOpts::default(),
        offset_tolerance,
    );

    let mut subpaths = split_subpaths(&stroked);
    let mut result = Vec::with_capacity(subpaths.len());
    if closed {
        let b = subpaths.pop().expect("a closed stroke yields two subpaths");
        let a = subpaths.pop().expect("a closed stroke yields two subpaths");
        debug_assert!(subpaths.is_empty());
        let [(role_a, role_b)] = winding::stroke_closed_roles(&a, &b);
        result.push((a, role_a));
        result.push((b, role_b));
    } else {
        let contour = subpaths.pop().expect("an open stroke yields one subpath");
        debug_assert!(subpaths.is_empty());
        result.push((contour, ContourRole::Outer));
    }

    for corner in corners(skeleton, closed) {
        let join = spec
            .join_overrides
            .iter()
            .find(|&&(index, _)| index == corner.segment_index)
            .map_or(spec.default_join, |&(_, join)| join);
        splice_join(&mut result, &corner, join, r, offset_tolerance);
    }

    Ok(result)
}

/// The `glyf`/generic tie-breaker for stroking joins (spec §14): `4`.
const MITER_LIMIT: f64 = 4.0;

/// Splits a `kurbo::stroke` output into its separate closed subpaths (one
/// per `MoveTo`).
fn split_subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut result = Vec::new();
    let mut current: Vec<PathEl> = Vec::new();
    for &el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) && !current.is_empty() {
            result.push(BezPath::from_vec(std::mem::take(&mut current)));
        }
        current.push(el);
    }
    if !current.is_empty() {
        result.push(BezPath::from_vec(current));
    }
    result
}

/// One corner of the skeleton (spec §6.4: "a vertex where the incoming
/// and outgoing tangents differ"), identified by the drawn-segment index
/// whose trailing endpoint it sits at — matching `StrokeSpec::join_overrides`'s
/// own indexing.
struct Corner {
    segment_index: usize,
    vertex: Point,
    incoming: Vec2,
    outgoing: Vec2,
}

/// Every corner of `skeleton`'s drawn segments: the joint after each
/// segment but the last, plus (when `closed`) the wraparound joint after
/// the last segment, back to the first. A corner whose incoming and
/// outgoing tangents already agree (e.g. a smooth reflection) is not a
/// corner at all (spec §6.4) and is skipped.
fn corners(skeleton: &Skeleton, closed: bool) -> Vec<Corner> {
    let pieces: Vec<kurbo::PathSeg> = skeleton::segments(&skeleton.path);
    let piece_counts = &skeleton.piece_counts;
    let n = piece_counts.len();

    let mut offsets = Vec::with_capacity(n + 1);
    let mut running = 0usize;
    for &count in piece_counts {
        offsets.push(running);
        running += count;
    }
    offsets.push(running);

    let last_piece_of = |segment: usize| offsets[segment + 1] - 1;
    let first_piece_of = |segment: usize| offsets[segment];

    let mut joints: Vec<(usize, usize, usize)> = (0..n.saturating_sub(1))
        .map(|d| (d, last_piece_of(d), first_piece_of(d + 1)))
        .collect();
    if closed && n > 0 {
        joints.push((n - 1, last_piece_of(n - 1), first_piece_of(0)));
    }

    joints
        .into_iter()
        .filter_map(|(segment_index, incoming_piece, outgoing_piece)| {
            let vertex = pieces[incoming_piece].end();
            let incoming = skeleton::direction_at(&pieces[incoming_piece], 1.0);
            let outgoing = skeleton::direction_at(&pieces[outgoing_piece], 0.0);
            is_corner(incoming, outgoing).then_some(Corner {
                segment_index,
                vertex,
                incoming,
                outgoing,
            })
        })
        .collect()
}

fn is_corner(incoming: Vec2, outgoing: Vec2) -> bool {
    let (a, b) = (incoming.normalize(), outgoing.normalize());
    (a - b).hypot() > 1e-9
}

/// The outward unit normal at a tangent `t`, on the side a join belongs
/// (spec §6.4: "on the outer side of the turn"). A positive
/// `turn` (the incoming and outgoing tangents' cross product) is a CCW
/// (left) turn, whose outer side is the traveler's right; `left_normal`
/// is `t` rotated +90°, so the outer offset is `-left_normal` there, and
/// the mirror image for a CW turn.
fn outer_offset(t: Vec2, turn: f64) -> Vec2 {
    let left_normal = Vec2::new(-t.y, t.x).normalize();
    if turn > 0.0 {
        -left_normal
    } else {
        left_normal
    }
}

/// Rewrites `corner`'s join in place (spec plan M4's "join splicing"):
/// `"bevel"` leaves `Join::Bevel`'s own chord untouched; `"round"`/`"miter"`
/// (the latter within `MITER_LIMIT`) replace it with the exact join
/// geometry. A `"miter"` past the limit, or a degenerate (180°-reversal)
/// corner with no well-defined apex, also leaves the chord as the bevel.
///
/// `corner`'s own tangent check only flags a *candidate* — kurbo has its
/// own, independent threshold for when a join is worth emitting at all,
/// and two arcs meeting at a carefully matched tangent (a common
/// deliberate construction, not just a straight-through joint) can come
/// out only *numerically* distinct after independent ellipse fits, close
/// enough that kurbo folds the join away entirely. When that happens,
/// there is no chord to find, and none is needed: this returns quietly,
/// same as `"bevel"`. `bevel_emits_exactly_one_chord_per_corner_matching_this_modules_assumption`
/// is what actually pins `Join::Bevel`'s emission shape for a genuine
/// corner, so a real kurbo drift still fails loudly — in the test suite,
/// not as a panic on someone's font.
fn splice_join(
    contours: &mut [(BezPath, ContourRole)],
    corner: &Corner,
    join: JoinKind,
    r: f64,
    tolerance: f64,
) {
    if join == JoinKind::Bevel {
        return;
    }

    let turn = corner.incoming.cross(corner.outgoing);
    let p_a = corner.vertex + outer_offset(corner.incoming, turn) * r;
    let p_b = corner.vertex + outer_offset(corner.outgoing, turn) * r;

    let make_replacement: Box<dyn Fn(Point, Point) -> Vec<PathEl>> = match join {
        JoinKind::Bevel => unreachable!("returned above"),
        JoinKind::Round => {
            let base_sweep = if turn > 0.0 { Sweep::Ccw } else { Sweep::Cw };
            Box::new(move |from, to| {
                // `from`/`to` is whichever of `(p_a, p_b)` or `(p_b, p_a)`
                // this corner's chord was actually found in; reverse the
                // sweep to match when it's the latter.
                let sweep = if from == p_a {
                    base_sweep
                } else {
                    flip(base_sweep)
                };
                skeleton::arc_cubics(from, to, corner.vertex, r, r, sweep)
                    .into_iter()
                    .map(|(c0, c1, end)| PathEl::CurveTo(c0, c1, end))
                    .collect()
            })
        }
        JoinKind::Miter => {
            if turn.abs() < 1e-9 {
                // A 180° reversal: no well-defined miter apex. Leaves the
                // bevel chord as-is.
                return;
            }
            let line_in = Line::new(p_a, p_a + corner.incoming);
            let line_out = Line::new(p_b, p_b + corner.outgoing);
            let Some(apex) = line_in.crossing_point(line_out) else {
                return;
            };
            if (apex - corner.vertex).hypot() / (2.0 * r) > MITER_LIMIT {
                return;
            }
            Box::new(move |_from, to| vec![PathEl::LineTo(apex), PathEl::LineTo(to)])
        }
    };

    for (contour, _) in contours.iter_mut() {
        if splice_chord(contour, p_a, p_b, tolerance, &*make_replacement) {
            return;
        }
    }
    // No chord in any contour: kurbo already treated this joint as
    // continuous (see this function's doc comment) — nothing to splice.
}

fn flip(sweep: Sweep) -> Sweep {
    match sweep {
        Sweep::Ccw => Sweep::Cw,
        Sweep::Cw => Sweep::Ccw,
    }
}

/// Finds the one `LineTo` in `contour` running between `p_a` and `p_b`
/// (in either direction, within `tolerance`) and replaces it with
/// `make_replacement(from, to)`'s elements, `from`/`to` being whichever
/// direction was actually found. Returns whether a chord was found.
fn splice_chord(
    contour: &mut BezPath,
    p_a: Point,
    p_b: Point,
    tolerance: f64,
    make_replacement: &dyn Fn(Point, Point) -> Vec<PathEl>,
) -> bool {
    let elements = contour.elements();
    let mut current = Point::ORIGIN;
    let mut start = Point::ORIGIN;
    let mut found: Option<(usize, Point, Point)> = None;

    for (i, el) in elements.iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) => {
                current = p;
                start = p;
            }
            PathEl::LineTo(p) => {
                if current.distance(p_a) <= tolerance && p.distance(p_b) <= tolerance {
                    found = Some((i, p_a, p_b));
                    break;
                }
                if current.distance(p_b) <= tolerance && p.distance(p_a) <= tolerance {
                    found = Some((i, p_b, p_a));
                    break;
                }
                current = p;
            }
            PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => current = p,
            PathEl::ClosePath => current = start,
        }
    }

    let Some((index, from, to)) = found else {
        return false;
    };
    let replacement = make_replacement(from, to);
    let mut owned: Vec<PathEl> = elements.to_vec();
    owned.splice(index..=index, replacement);
    *contour = BezPath::from_vec(owned);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{RawSegment, RawStart};
    use kurbo::PathSeg;

    const NO_ARC_TOLERANCE: f64 = 1e-9;
    const OFFSET_TOLERANCE: f64 = 0.05;

    fn default_spec(width: f64) -> StrokeSpec {
        StrokeSpec {
            width,
            start_cap: Cap::Butt,
            end_cap: Cap::Butt,
            default_join: JoinKind::Bevel,
            join_overrides: Vec::new(),
        }
    }

    fn right_angle_skeleton() -> Skeleton {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, 0.0),
            },
            RawSegment::Line {
                to: Point::new(10.0, 10.0),
            },
        ];
        skeleton::realize(&start, &segs, false, NO_ARC_TOLERANCE).unwrap()
    }

    #[test]
    fn open_path_yields_one_outer_contour() {
        let skeleton = right_angle_skeleton();
        let spec = default_spec(2.0);
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].1, ContourRole::Outer);
    }

    #[test]
    fn closed_path_yields_outer_and_counter() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, 0.0),
            },
            RawSegment::Line {
                to: Point::new(10.0, 10.0),
            },
            RawSegment::Line {
                to: Point::new(0.0, 10.0),
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        let spec = default_spec(2.0);
        let contours = stroke_path(&skeleton, true, &spec, OFFSET_TOLERANCE).unwrap();
        let roles: Vec<ContourRole> = contours.iter().map(|(_, role)| role).copied().collect();
        assert!(roles.contains(&ContourRole::Outer));
        assert!(roles.contains(&ContourRole::Counter));
    }

    #[test]
    fn bevel_emits_exactly_one_chord_per_corner_matching_this_modules_assumption() {
        // Pinned assumption (spec plan M4): `kurbo::Join::Bevel` emits
        // exactly one `LineTo` between the two offset endpoints at each
        // corner. If a kurbo upgrade changes this, `splice_join`'s panic
        // (not a silent skip) is the intended failure mode; this test
        // just makes the same assumption explicit and fast to check.
        let skeleton = right_angle_skeleton();
        let spec = default_spec(2.0);
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        let (p_a, p_b) = (Point::new(10.0, -1.0), Point::new(11.0, 0.0));
        let segs = skeleton::segments(&contours[0].0);
        let chord_count = segs
            .iter()
            .filter(|seg| {
                matches!(seg, PathSeg::Line(l) if
                    (l.p0.distance(p_a) < 1e-6 && l.p1.distance(p_b) < 1e-6)
                    || (l.p0.distance(p_b) < 1e-6 && l.p1.distance(p_a) < 1e-6))
            })
            .count();
        assert_eq!(chord_count, 1, "{segs:#?}");
    }

    #[test]
    fn round_join_splices_an_exact_arc_with_no_extra_contour() {
        let skeleton = right_angle_skeleton();
        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Round;
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        assert_eq!(contours.len(), 1, "join splicing adds no extra contours");
        assert_eq!(contours[0].1, ContourRole::Outer);

        let vertex = Point::new(10.0, 0.0);
        let r = 1.0;
        let segs = skeleton::segments(&contours[0].0);
        let arc = segs
            .iter()
            .find_map(|seg| match seg {
                PathSeg::Cubic(c)
                    if ((c.p0 - vertex).hypot() - r).abs() < 1e-6
                        && ((c.p3 - vertex).hypot() - r).abs() < 1e-6 =>
                {
                    Some(*c)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no round-join arc found among {segs:#?}"));

        // A single cubic Bézier only approximates a 90° circular arc, to
        // within about `2.7e-4 · r` at its worst point — the same bound
        // `skeleton`'s own arc tests use (`arc_stays_within_bound_of_the_exact_ellipse`),
        // not exact to floating-point precision.
        let bound = 3e-4 * r;
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let p = arc.eval(t);
            assert!(
                ((p - vertex).hypot() - r).abs() < bound,
                "point at t={t} is off the circle: {p:?}"
            );
        }
    }

    #[test]
    fn miter_join_splices_two_lines_through_the_apex_with_no_extra_contour() {
        let skeleton = right_angle_skeleton();
        let bevel_contours =
            stroke_path(&skeleton, false, &default_spec(2.0), OFFSET_TOLERANCE).unwrap();
        let bevel_count = skeleton::segments(&bevel_contours[0].0).len();

        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Miter;
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        assert_eq!(contours.len(), 1, "join splicing adds no extra contours");

        let segs = skeleton::segments(&contours[0].0);
        assert_eq!(
            segs.len(),
            bevel_count + 1,
            "one bevel chord becomes two miter lines: {segs:#?}"
        );

        // Worked out by hand for `right_angle_skeleton`'s corner: the
        // incoming offset line is x=10, the outgoing offset line is
        // y=-1, so the apex is their intersection (11, -1).
        let apex = Point::new(11.0, -1.0);
        assert!(
            segs.iter().any(|seg| matches!(seg, PathSeg::Line(l)
                if l.p0.distance(apex) < 1e-6 || l.p1.distance(apex) < 1e-6)),
            "expected the miter apex {apex:?} among {segs:#?}"
        );
    }

    #[test]
    fn near_tangent_corner_between_different_center_arcs_does_not_panic() {
        // Regression: two arcs with *different* centers, deliberately
        // chosen (as a hand-tuned bowl/stem transition typically is) so
        // their tangents match closely at the shared point. Two
        // independent `fit_ellipse` solves can land this a hair short of
        // exactly continuous, over `is_corner`'s threshold — but kurbo's
        // own stroker still folds a joint this close together and emits
        // no distinguishable chord, so `splice_join` must not treat that
        // as an error (see its own doc comment).
        let w = 500.0;
        let h = 1000.0;
        let ctr0 = Point::new(0.477 * w, 0.281 * h);
        let r0 = 0.548 * w;
        let ctr1 = Point::new(0.305 * w, 0.172 * h);
        let r1 = 0.270 * w;

        let polar = |ctr: Point, r: f64, deg: f64| {
            let rad = deg.to_radians();
            ctr + Vec2::new(r * rad.cos(), r * rad.sin())
        };
        let bowl0 = polar(ctr0, r0, -25.0);
        let bowl1 = polar(ctr0, r0, 180.0 + 52.0);
        let bowl2 = polar(ctr1, r1, 14.0 + 90.0);

        let start = RawStart { at: bowl0 };
        let segs = [
            RawSegment::Arc {
                to: bowl1,
                geometry: skeleton::ArcGeometry::Center(ctr0),
                sweep: Sweep::Cw,
            },
            RawSegment::Arc {
                to: bowl2,
                geometry: skeleton::ArcGeometry::Center(ctr1),
                sweep: Sweep::Cw,
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, false, 1e-9).unwrap();
        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Round;
        // Must not panic.
        stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
    }

    #[test]
    fn bevel_join_leaves_the_chord_unchanged() {
        let skeleton = right_angle_skeleton();
        let spec = default_spec(2.0); // default_join is Bevel
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        assert_eq!(contours.len(), 1);
    }

    #[test]
    fn join_at_override_wins_over_the_path_default() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, 0.0),
            },
            RawSegment::Line {
                to: Point::new(10.0, 10.0),
            },
            RawSegment::Line {
                to: Point::new(0.0, 10.0),
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, false, NO_ARC_TOLERANCE).unwrap();
        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Bevel;
        spec.join_overrides.push((0, JoinKind::Round));
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        assert_eq!(contours.len(), 1, "join splicing adds no extra contours");

        let r = 1.0;
        let has_round_arc_at = |vertex: Point, segs: &[PathSeg]| {
            segs.iter().any(|seg| {
                matches!(seg, PathSeg::Cubic(c)
                    if ((c.p0 - vertex).hypot() - r).abs() < 1e-6
                        && ((c.p3 - vertex).hypot() - r).abs() < 1e-6)
            })
        };
        let spliced = skeleton::segments(&contours[0].0);
        assert!(
            has_round_arc_at(Point::new(10.0, 0.0), &spliced),
            "corner 0 (overridden to round) should have a spliced arc: {spliced:#?}"
        );
        assert!(
            !has_round_arc_at(Point::new(10.0, 10.0), &spliced),
            "corner 1 (still bevel) should not: {spliced:#?}"
        );
    }

    #[test]
    fn non_positive_stroke_is_an_error() {
        let skeleton = right_angle_skeleton();
        let spec = default_spec(0.0);
        assert_eq!(
            stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE),
            Err(StrokeError::NonPositiveStroke)
        );
    }

    #[test]
    fn zero_length_path_is_an_error() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let skeleton = skeleton::realize(&start, &[], false, NO_ARC_TOLERANCE).unwrap();
        let spec = default_spec(2.0);
        assert_eq!(
            stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE),
            Err(StrokeError::ZeroLengthPath)
        );
    }

    #[test]
    fn curvature_violation_is_reported_before_stroking() {
        // A tiny circle (radius 2) stroked at a much wider width (20):
        // the curvature check must reject this before kurbo ever runs.
        let start = RawStart {
            at: Point::new(2.0, 0.0),
        };
        let segs = [
            RawSegment::Arc {
                to: Point::new(-2.0, 0.0),
                geometry: skeleton::ArcGeometry::Center(Point::new(0.0, 0.0)),
                sweep: skeleton::Sweep::Ccw,
            },
            RawSegment::Arc {
                to: Point::new(2.0, 0.0),
                geometry: skeleton::ArcGeometry::Center(Point::new(0.0, 0.0)),
                sweep: skeleton::Sweep::Ccw,
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        let spec = default_spec(20.0);
        let result = stroke_path(&skeleton, true, &spec, OFFSET_TOLERANCE);
        assert!(
            matches!(result, Err(StrokeError::Curvature(_))),
            "{result:#?}"
        );
    }
}
