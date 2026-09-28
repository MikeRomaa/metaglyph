//! Stroking (spec §6.4, §7). Offset generation itself is `kurbo::stroke`
//! (spec plan M4's stroker decision); what this module adds is everything
//! kurbo can't do on its own:
//! - the spec §7.2/§7.3 checks that must run *before* kurbo is called,
//! - join patches, since kurbo takes one join style for the whole path
//!   and `joinAt` needs a different join at one vertex (spec plan M4:
//!   "stroking with `Join::Bevel` and adding overlap patches"),
//! - assigning each output contour its role (spec §8.1), which kurbo's
//!   subpath emission order does not track.

use kurbo::{BezPath, Circle, Line, ParamCurve, PathEl, Point, Shape, Vec2};

use crate::curvature::{self, CurvatureViolation};
use crate::skeleton::{self, Skeleton};
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
/// path, an (outer, counter) pair for a closed one, plus any join
/// patches — always outer (spec: "Patches are outer-role contours").
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
    let mut result = Vec::with_capacity(subpaths.len() + 2);
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
        if let Some(patch) = build_patch(corner, join, r) {
            result.push((patch, ContourRole::Outer));
        }
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

/// The outward unit normal at a tangent `t`, on the side the join patch
/// belongs (spec §6.4: "on the outer side of the turn"). A positive
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

/// The join patch for `corner` (spec §6.4), or `None` for `"bevel"` (the
/// base stroke already is one) or a `"miter"` past `MITER_LIMIT` (falls
/// back to bevel, i.e. also nothing extra to add).
fn build_patch(corner: Corner, join: JoinKind, r: f64) -> Option<BezPath> {
    match join {
        JoinKind::Bevel => None,
        JoinKind::Round => Some(Circle::new(corner.vertex, r).to_path(0.1 * r)),
        JoinKind::Miter => build_miter_patch(&corner, r),
    }
}

fn build_miter_patch(corner: &Corner, r: f64) -> Option<BezPath> {
    let turn = corner.incoming.cross(corner.outgoing);
    if turn.abs() < 1e-9 {
        // Tangents point in opposite directions (a 180° reversal): no
        // well-defined miter apex. Falls back to the base bevel.
        return None;
    }

    let offset_in = outer_offset(corner.incoming, turn) * r;
    let offset_out = outer_offset(corner.outgoing, turn) * r;
    let p_a = corner.vertex + offset_in;
    let p_b = corner.vertex + offset_out;

    let line_a = Line::new(p_a, p_a + corner.incoming);
    let line_b = Line::new(p_b, p_b + corner.outgoing);
    let apex = line_a.crossing_point(line_b)?;

    let miter_length = (apex - corner.vertex).hypot();
    if miter_length / (2.0 * r) > MITER_LIMIT {
        return None;
    }

    let mut patch = BezPath::new();
    patch.move_to(p_a);
    patch.line_to(apex);
    patch.line_to(p_b);
    patch.close_path();
    Some(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{RawSegment, RawStart};

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
    fn round_join_adds_a_circular_patch() {
        let skeleton = right_angle_skeleton();
        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Round;
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        // The base outer contour, plus one round-join patch at the corner.
        assert_eq!(contours.len(), 2);
        assert!(contours.iter().all(|(_, role)| *role == ContourRole::Outer));
    }

    #[test]
    fn miter_join_adds_a_triangular_patch_within_limit() {
        let skeleton = right_angle_skeleton();
        let mut spec = default_spec(2.0);
        spec.default_join = JoinKind::Miter;
        let contours = stroke_path(&skeleton, false, &spec, OFFSET_TOLERANCE).unwrap();
        // A 90-degree corner's miter ratio (~1.41) is well within the
        // default limit of 4, so a patch is added.
        assert_eq!(contours.len(), 2);
    }

    #[test]
    fn bevel_join_adds_no_patch() {
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
        // Corner 0 (overridden to round) gets a patch; corner 1 (still
        // bevel) does not.
        assert_eq!(contours.len(), 2);
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
