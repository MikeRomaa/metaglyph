//! Tight curvature (spec §7.2), found before offset generation: within
//! the interior of any skeleton segment, where the curvature radius is
//! smaller than `r = stroke/2`, the offset on the concave side folds back.
//! Those intervals are *folds*: `crate::stroke` trims them after stroking
//! and reports them as warnings. Corners between authored segments are
//! excluded — joins handle them (`crate::stroke`) — but the joints
//! between one `arc`'s own cubic pieces are interior to that segment and
//! are checked here too. A cusp (vanishing derivative) anywhere interior
//! is not a fold but an error (spec §7.3): its offset has no direction.
//!
//! Kurbo has no analytic curvature-extremum solver, so this finds each
//! piece's worst point by dense sampling followed by golden-section
//! refinement of the best bracket — adequate given curvature is smooth
//! within one Bézier piece and the check only needs to compare its peak
//! against a threshold, not report an exact extremum.

use std::ops::Range;

use kurbo::{ParamCurve, PathSeg, Point, Vec2};

use crate::skeleton::{self, Skeleton};

/// An interval of one authored segment, in that segment's own local
/// `[0, 1]` domain — the same domain `crate::skeleton::authored_param_to_piece`
/// indexes into for segment `segment_index`, i.e. add `segment_index` to
/// get the spec §5.9 path-query global parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvatureViolation {
    pub segment_index: usize,
    pub local_t: Range<f64>,
}

/// A fold (spec §7.2): where the curvature radius is below `r`, and the
/// point of the concave-side offset at its tightest spot, `p + r·n̂`
/// toward the centre of curvature — which lies on the offset's reversed
/// loop that `crate::stroke` trims.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    pub at: CurvatureViolation,
    pub anchor: Point,
}

/// The number of samples used to bracket each piece's curvature maximum
/// before refining it. Cheap relative to a typical glyph's segment count,
/// and the refinement step below is what actually delivers precision.
const SAMPLES: usize = 64;

/// Below this speed (`|p′(t)|`), the derivative has vanished: a cusp.
const CUSP_SPEED: f64 = 1e-9;

/// Every fold of `skeleton` at stroke radius `r`, in segment order, or
/// the first interior cusp (spec §7.3), which is an error.
pub fn folds(skeleton: &Skeleton, r: f64) -> Result<Vec<Fold>, CurvatureViolation> {
    let pieces: Vec<PathSeg> = skeleton::segments(&skeleton.path);
    let threshold = 1.0 / r;

    let mut out = Vec::new();
    let mut offset = 0usize;
    for (segment_index, &piece_count) in skeleton.piece_counts.iter().enumerate() {
        let segment_pieces = &pieces[offset..offset + piece_count];
        for (local_t, peak) in segment_folds(segment_pieces, threshold) {
            let at = CurvatureViolation {
                segment_index,
                local_t,
            };
            let (piece, t) = peak;
            let seg = &segment_pieces[piece];
            let direction = skeleton::direction_at(seg, t);
            if direction.hypot() < CUSP_SPEED {
                return Err(at);
            }
            let left = Vec2::new(-direction.y, direction.x).normalize();
            let side = skeleton::curvature_at(seg, t).signum();
            out.push(Fold {
                at,
                anchor: seg.eval(t) + left * (r * side),
            });
        }
        offset += piece_count;
    }
    Ok(out)
}

/// One authored segment's folds: each piece's worst interior point, plus
/// (spec §7.2) the joints between consecutive pieces of the same `arc`,
/// which are interior to the segment even though they sit at a piece
/// boundary. The segment's two true endpoints — shared with its
/// neighbors — are never touched here. Intervals that meet across a
/// joint (an ellipse vertex where two pieces join) merge into one fold;
/// each comes with its tightest point as `(piece, t)`.
fn segment_folds(pieces: &[PathSeg], threshold: f64) -> Vec<(Range<f64>, (usize, f64))> {
    let piece_count = pieces.len();
    let mut out: Vec<(Range<f64>, (usize, f64), f64)> = Vec::new();
    let mut push = |range: Range<f64>, peak: (usize, f64), k: f64| {
        if let Some((last, last_peak, last_k)) = out.last_mut()
            && range.start <= last.end + 1e-9
        {
            last.end = last.end.max(range.end);
            if k > *last_k {
                *last_peak = peak;
                *last_k = k;
            }
            return;
        }
        out.push((range, peak, k));
    };

    for (k, piece) in pieces.iter().enumerate() {
        let (t_peak, worst) = worst_interior_point(piece);
        let joint = (k + 1 < piece_count)
            .then(|| curvature_or_infinite(piece, 1.0))
            .filter(|&c| c > threshold);
        if worst > threshold {
            let (lo, hi) = violation_bounds(piece, t_peak, threshold);
            push(
                piece_local_to_segment_local(k, piece_count, lo)
                    ..piece_local_to_segment_local(k, piece_count, hi),
                (k, t_peak),
                worst,
            );
        }
        if let Some(c) = joint {
            let t = piece_local_to_segment_local(k, piece_count, 1.0);
            push(t..t, (k, 1.0), c);
        }
    }
    out.into_iter()
        .map(|(range, peak, _)| (range, peak))
        .collect()
}

fn piece_local_to_segment_local(piece_index: usize, piece_count: usize, local_t: f64) -> f64 {
    (piece_index as f64 + local_t) / piece_count as f64
}

/// The curvature magnitude at `t`, or `f64::INFINITY` where the
/// derivative vanishes (a cusp, spec §7.3) — guarding this explicitly
/// rather than trusting the curvature formula's `0/0` to come out as a
/// large-enough finite number.
fn curvature_or_infinite(seg: &PathSeg, t: f64) -> f64 {
    let speed = skeleton::direction_at(seg, t).hypot();
    if speed < CUSP_SPEED {
        return f64::INFINITY;
    }
    skeleton::curvature_at(seg, t).abs()
}

/// The worst (highest-magnitude) curvature strictly inside `(0, 1)`,
/// found by sampling to bracket the peak and golden-section search to
/// refine it.
fn worst_interior_point(seg: &PathSeg) -> (f64, f64) {
    let mut best_t = 0.5;
    let mut best_k = curvature_or_infinite(seg, best_t);
    for i in 1..SAMPLES {
        let t = i as f64 / SAMPLES as f64;
        let k = curvature_or_infinite(seg, t);
        if k > best_k {
            best_k = k;
            best_t = t;
        }
    }

    let step = 1.0 / SAMPLES as f64;
    let lo = (best_t - step).max(1e-9);
    let hi = (best_t + step).min(1.0 - 1e-9);
    golden_section_max(seg, lo, hi)
}

const INV_GOLDEN: f64 = 0.6180339887498949;

/// Golden-section search for the `t` maximizing curvature within
/// `[lo, hi]`, assuming (as holds for a Bézier piece near one local
/// extremum) that the function is unimodal on this bracket.
fn golden_section_max(seg: &PathSeg, mut lo: f64, mut hi: f64) -> (f64, f64) {
    let mut c = hi - INV_GOLDEN * (hi - lo);
    let mut d = lo + INV_GOLDEN * (hi - lo);
    let mut fc = curvature_or_infinite(seg, c);
    let mut fd = curvature_or_infinite(seg, d);

    for _ in 0..60 {
        if (hi - lo).abs() < 1e-13 {
            break;
        }
        if fc > fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - INV_GOLDEN * (hi - lo);
            fc = curvature_or_infinite(seg, c);
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + INV_GOLDEN * (hi - lo);
            fd = curvature_or_infinite(seg, d);
        }
    }

    let t = (lo + hi) / 2.0;
    (t, curvature_or_infinite(seg, t))
}

/// The sub-interval around `t_peak` (itself known to violate `threshold`)
/// where curvature stays above it, found by bisecting outward from the
/// peak against the piece's own two endpoints.
fn violation_bounds(seg: &PathSeg, t_peak: f64, threshold: f64) -> (f64, f64) {
    let lo = bisect_edge(seg, 0.0, t_peak, threshold);
    let hi = bisect_edge(seg, 1.0, t_peak, threshold);
    (lo, hi)
}

/// Bisects between `safe` (assumed at or below `threshold`) and `bad`
/// (known above it) for the crossing point, assuming — adequate for a
/// diagnostic's reported interval, not a certified bound — a single
/// monotonic transition between them.
fn bisect_edge(seg: &PathSeg, safe: f64, bad: f64, threshold: f64) -> f64 {
    if curvature_or_infinite(seg, safe) > threshold {
        return safe;
    }
    let (mut lo, mut hi) = (safe, bad);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if curvature_or_infinite(seg, mid) > threshold {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{RawSegment, RawStart, Sweep};
    use kurbo::Point;

    const NO_ARC_TOLERANCE: f64 = 1e-9;

    fn circle_skeleton(radius: f64) -> Skeleton {
        // A full circle needs at least two arcs (spec §6.3); two
        // half-circles are enough for this module's purposes.
        let start = RawStart {
            at: Point::new(radius, 0.0),
        };
        let segs = [
            RawSegment::Arc {
                to: Point::new(-radius, 0.0),
                geometry: skeleton::ArcGeometry::Center(Point::new(0.0, 0.0)),
                sweep: Sweep::Ccw,
            },
            RawSegment::Arc {
                to: Point::new(radius, 0.0),
                geometry: skeleton::ArcGeometry::Center(Point::new(0.0, 0.0)),
                sweep: Sweep::Ccw,
            },
        ];
        skeleton::realize(&start, &segs, true, NO_ARC_TOLERANCE).unwrap()
    }

    #[test]
    fn a_wide_circle_passes_a_narrow_stroke() {
        let skeleton = circle_skeleton(50.0);
        // Curvature radius is exactly 50 everywhere; a stroke radius of
        // 10 (well under 50) must never fold.
        assert_eq!(folds(&skeleton, 10.0), Ok(vec![]));
    }

    #[test]
    fn a_narrow_circle_fails_a_wide_stroke() {
        let skeleton = circle_skeleton(5.0);
        // Curvature radius is exactly 5 everywhere; a stroke radius of 20
        // (far over 5) must violate, including at the arc's own internal
        // piece joints (a circle's curvature is uniform, so any point
        // works as the witness).
        let found = folds(&skeleton, 20.0).unwrap();
        // One fold per arc, each spanning the whole arc (its piece
        // joints merge), anchored on the concave side: the centre side,
        // 20 in from a radius-5 circle, so 15 past the centre.
        assert_eq!(found.len(), 2, "{found:?}");
        for fold in &found {
            assert!(
                fold.at.local_t.start < 0.01 && fold.at.local_t.end > 0.99,
                "{fold:?}"
            );
            assert!(
                (fold.anchor.to_vec2().hypot() - 15.0).abs() < 0.1,
                "{fold:?}"
            );
        }
    }

    #[test]
    fn curvature_is_positive_turning_counter_clockwise() {
        // spec §5.9 `curvatureAt`: a CCW circle of radius 5 has +1/5,
        // within the cubic approximation's own error.
        let skeleton = circle_skeleton(5.0);
        let piece = skeleton::segments(&skeleton.path)[0];
        assert!((skeleton::curvature_at(&piece, 0.5) - 0.2).abs() < 0.005);
    }

    #[test]
    fn an_ellipse_folds_only_at_its_tight_vertices() {
        // rx 60, ry 120: the curvature radius is rx²/ry = 30 at the top and
        // bottom, ry²/rx = 240 at the sides. A stroke radius of 50 folds
        // the top and bottom only, anchored 50 below the top and 50 above
        // the bottom.
        let at = |deg: f64| {
            Point::new(
                60.0 * deg.to_radians().cos(),
                120.0 * deg.to_radians().sin(),
            )
        };
        let start = RawStart { at: at(0.0) };
        let arc = |to| RawSegment::Arc {
            to,
            geometry: skeleton::ArcGeometry::Radii {
                rx: 60.0,
                ry: 120.0,
                large: false,
            },
            sweep: Sweep::Ccw,
        };
        let skeleton = skeleton::realize(
            &start,
            &[arc(at(180.0)), arc(at(0.0))],
            true,
            NO_ARC_TOLERANCE,
        )
        .unwrap();
        let found = folds(&skeleton, 50.0).unwrap();
        assert_eq!(found.len(), 2, "{found:?}");
        let anchors: Vec<Point> = found.iter().map(|f| f.anchor).collect();
        // Within the arc's cubic approximation, whose tightest point sits
        // a little off the true vertex.
        assert!(
            anchors[0].distance(Point::new(0.0, 70.0)) < 1.5,
            "{anchors:?}"
        );
        assert!(
            anchors[1].distance(Point::new(0.0, -70.0)) < 1.5,
            "{anchors:?}"
        );
        // Symmetric about each arc's middle.
        let t = &found[0].at.local_t;
        assert!((t.start + t.end - 1.0).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn a_line_never_violates_any_stroke_width() {
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [RawSegment::Line {
            to: Point::new(100.0, 0.0),
        }];
        let skeleton = skeleton::realize(&start, &segs, false, NO_ARC_TOLERANCE).unwrap();
        // A line has zero curvature everywhere: no stroke width, however
        // large, can fold it.
        assert_eq!(folds(&skeleton, 1e6), Ok(vec![]));
    }

    #[test]
    fn a_cusp_violates_any_positive_stroke_width() {
        // Control points chosen so the cubic's derivative (itself a
        // quadratic Bézier over 3(P1-P0), 3(P2-P1), 3(P3-P2)) hits exactly
        // zero at t=0.5: a genuine interior cusp, verified algebraically,
        // not just eyeballed.
        let start = RawStart {
            at: Point::new(0.0, 0.0),
        };
        let segs = [RawSegment::Cube {
            to: Point::new(0.5, 0.5),
            c1: Some(Point::new(1.0, 0.0)),
            c2: Point::new(0.5, -0.5),
        }];
        let skeleton = skeleton::realize(&start, &segs, false, NO_ARC_TOLERANCE).unwrap();
        // This configuration's derivative vanishes inside (0, 1); any
        // positive stroke width must be rejected.
        assert!(folds(&skeleton, 1.0).is_err());
    }
}
