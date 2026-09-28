//! Filled paths (spec §6.5, §8.3). `fill: true` inks a closed skeleton's
//! interior directly — no offsetting, so `crate::stroke`'s checks don't
//! apply — except for one hard error of its own: a filled contour must
//! not cross itself.

use kurbo::{BezPath, PathSeg};

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

/// A crossing this close (in either segment's own local parameter) to
/// `0` or `1` is the shared endpoint of two adjacent segments, not a
/// genuine self-intersection (spec §8.3: "excluding the shared endpoints
/// of adjacent segments").
const ENDPOINT_EPSILON: f64 = 1e-6;

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
            // Each entry is `(t on piece i, t on piece j)` at a joint the
            // two share. A two-piece contour shares both.
            let mut shared_joints: Vec<(f64, f64)> = Vec::new();
            if j == i + 1 {
                shared_joints.push((1.0, 0.0));
            }
            if i == 0 && j == n - 1 {
                shared_joints.push((0.0, 1.0));
            }

            for hit in intersect::segment_intersections(pieces[i], pieces[j], tolerance) {
                let at_joint = shared_joints.iter().any(|&(at_a, at_b)| {
                    (hit.t_a - at_a).abs() < ENDPOINT_EPSILON
                        && (hit.t_b - at_b).abs() < ENDPOINT_EPSILON
                });
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
}
