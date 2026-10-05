//! Filled paths (spec §6.5, §8.3). `fill: true` inks a closed skeleton's
//! interior directly — no offsetting, so `crate::stroke`'s checks don't
//! apply — except for one hard error of its own: a filled contour must
//! not cross itself.

use kurbo::{BezPath, ParamCurve, PathSeg};

use crate::intersect;
use crate::skeleton::{self, Skeleton};

/// A self-intersecting filled contour (spec §8.3): each crossing's two
/// segments' own spec §5.9 path-query parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SelfIntersection {
    pub crossings: Vec<(f64, f64)>,
}

/// The filled contour: the skeleton's own path, unchanged (spec §6.5:
/// "The path's own skeleton is the contour; no offsetting happens").
pub fn fill_contour(skeleton: &Skeleton) -> BezPath {
    skeleton.path.clone()
}

/// A crossing between adjacent pieces this close to their shared joint —
/// as a fraction of the shorter piece's size — is the joint itself, not a
/// genuine self-intersection (spec §8.3: "excluding the shared endpoints
/// of adjacent segments"). Judged by distance, not parameter: where two
/// pieces meet tangentially (an arc's own cubic pieces), they stay within
/// the solver's tolerance of each other for a short way past the joint,
/// and it reports a hit a hair off it.
const JOINT_FRACTION: f64 = 1e-3;

/// A piece's size: its control polygon's length.
fn piece_size(seg: PathSeg) -> f64 {
    match seg {
        PathSeg::Line(l) => l.p0.distance(l.p1),
        PathSeg::Quad(q) => q.p0.distance(q.p1) + q.p1.distance(q.p2),
        PathSeg::Cubic(c) => c.p0.distance(c.p1) + c.p1.distance(c.p2) + c.p2.distance(c.p3),
    }
}

/// Checks `skeleton` — already known closed, since `fill` requires it —
/// for self-intersection (spec §8.3), reporting each crossing in spec
/// §5.9 path-query parameters.
pub fn check_self_intersection(
    skeleton: &Skeleton,
    tolerance: f64,
) -> Result<(), SelfIntersection> {
    let pieces: Vec<PathSeg> = skeleton::segments(&skeleton.path);
    let crossings: Vec<(f64, f64)> = contour_self_intersections(&pieces, tolerance)
        .into_iter()
        .map(|hit| {
            (
                skeleton::piece_to_authored_param(skeleton, hit.a, hit.t_a),
                skeleton::piece_to_authored_param(skeleton, hit.b, hit.t_b),
            )
        })
        .collect();

    if crossings.is_empty() {
        Ok(())
    } else {
        Err(SelfIntersection { crossings })
    }
}

/// One crossing between two pieces `a < b` of a closed contour, as each
/// piece's index and its own local `t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PieceCrossing {
    pub a: usize,
    pub t_a: f64,
    pub b: usize,
    pub t_b: f64,
}

/// Every crossing among the pieces of one closed contour (spec §8.3):
/// every pair, excluding each adjacent pair's shared joint — including
/// the wraparound pair, last against first, which shares the start
/// point. Shared by the realized-skeleton check above and `mg-font`'s
/// re-check of a filled contour after quantization (spec §10.4).
pub fn contour_self_intersections(pieces: &[PathSeg], tolerance: f64) -> Vec<PieceCrossing> {
    let n = pieces.len();
    let mut crossings = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            // The joints the two share: piece `i`'s end, where `j` starts,
            // and (all the way round) its start, where `j` ends. A
            // two-piece contour shares both.
            let mut shared_joints = Vec::new();
            if j == i + 1 {
                shared_joints.push(pieces[i].end());
            }
            if i == 0 && j == n - 1 {
                shared_joints.push(pieces[i].start());
            }
            let reach = (JOINT_FRACTION * piece_size(pieces[i]).min(piece_size(pieces[j])))
                .max(tolerance);

            for hit in intersect::segment_intersections(pieces[i], pieces[j], tolerance) {
                let at = pieces[i].eval(hit.t_a.clamp(0.0, 1.0));
                let at_joint = shared_joints.iter().any(|joint| joint.distance(at) <= reach);
                if !at_joint {
                    crossings.push(PieceCrossing {
                        a: i,
                        t_a: hit.t_a,
                        b: j,
                        t_b: hit.t_b,
                    });
                }
            }
        }
    }

    crossings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{RawSegment, RawStart};
    use kurbo::Point;

    const NO_ARC_TOLERANCE: f64 = 1e-9;
    const TOLERANCE: f64 = 1e-3;

    #[test]
    fn a_simple_square_does_not_self_intersect() {
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
        assert_eq!(check_self_intersection(&skeleton, TOLERANCE), Ok(()));
    }

    #[test]
    fn a_figure_eight_self_intersects() {
        // Two triangles sharing only their apex at the origin, forming a
        // bowtie: the two non-adjacent diagonal legs cross.
        let start = RawStart {
            at: Point::new(-10.0, 10.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, -10.0),
            },
            RawSegment::Line {
                to: Point::new(10.0, 10.0),
            },
            RawSegment::Line {
                to: Point::new(-10.0, -10.0),
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        let result = check_self_intersection(&skeleton, TOLERANCE);
        assert!(result.is_err(), "{result:#?}");
        let crossings = result.unwrap_err().crossings;
        assert_eq!(crossings.len(), 1);
    }

    #[test]
    fn adjacent_segments_shared_endpoint_is_not_a_crossing() {
        // A plain triangle: consecutive segments only ever meet at their
        // shared vertex, never a genuine crossing.
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, 0.0),
            },
            RawSegment::Line {
                to: Point::new(5.0, 10.0),
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        assert_eq!(check_self_intersection(&skeleton, TOLERANCE), Ok(()));
    }

    #[test]
    fn fill_contour_is_the_skeleton_unchanged() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [
            RawSegment::Line {
                to: Point::new(10.0, 0.0),
            },
            RawSegment::Line {
                to: Point::new(5.0, 10.0),
            },
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        assert_eq!(fill_contour(&skeleton), skeleton.path);
    }

    #[test]
    fn arc_piece_joints_are_not_crossings() {
        // The `!` in samples/a22x-mono.mg, filled: a large, narrow ellipse
        // arc (three cubic pieces), a line, a small arc (two pieces), and
        // the closing line. The arcs' pieces meet tangentially, which the
        // intersection solver can report a hair off the joint itself.
        let polar = |c: Point, rx: f64, ry: f64, deg: f64| {
            let (cos, sin) = (deg.to_radians().cos(), deg.to_radians().sin());
            let t = 1.0 / ((cos / rx).powi(2) + (sin / ry).powi(2)).sqrt();
            Point::new(c.x + t * cos, c.y + t * sin)
        };
        let (upper, lower) = (Point::new(250.0, 880.0), Point::new(250.0, 500.0));
        let arc = |to, rx, ry, large| RawSegment::Arc {
            to,
            geometry: skeleton::ArcGeometry::Radii { rx, ry, large },
            sweep: skeleton::Sweep::Cw,
        };
        let start = RawStart {
            at: polar(upper, 60.0, 120.0, 185.0),
        };
        let segs = [
            arc(polar(upper, 60.0, 120.0, -5.0), 60.0, 120.0, true),
            RawSegment::Line {
                to: polar(lower, 25.0, 50.0, -5.0),
            },
            arc(polar(lower, 25.0, 50.0, 185.0), 25.0, 50.0, false),
        ];
        let skeleton = skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap();
        assert_eq!(check_self_intersection(&skeleton, 1e-6), Ok(()));
    }
}
