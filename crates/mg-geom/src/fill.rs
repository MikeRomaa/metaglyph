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
/// for self-intersection (spec §8.3): every pair of its own underlying
/// segments, excluding each adjacent pair's shared joint (and the
/// wraparound pair, first against last, which shares the start point).
pub fn check_self_intersection(
    skeleton: &Skeleton,
    tolerance: f64,
) -> Result<(), SelfIntersection> {
    let pieces: Vec<PathSeg> = skeleton::segments(&skeleton.path);
    let n = pieces.len();
    let mut crossings = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let shared_joint = if j == i + 1 {
                Some((1.0, 0.0))
            } else if i == 0 && j == n - 1 {
                Some((0.0, 1.0))
            } else {
                None
            };

            for hit in intersect::segment_intersections(pieces[i], pieces[j], tolerance) {
                if let Some((at_a, at_b)) = shared_joint
                    && (hit.t_a - at_a).abs() < ENDPOINT_EPSILON
                    && (hit.t_b - at_b).abs() < ENDPOINT_EPSILON
                {
                    continue;
                }
                crossings.push((
                    skeleton::piece_to_authored_param(skeleton, i, hit.t_a),
                    skeleton::piece_to_authored_param(skeleton, j, hit.t_b),
                ));
            }
        }
    }

    if crossings.is_empty() {
        Ok(())
    } else {
        Err(SelfIntersection { crossings })
    }
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
