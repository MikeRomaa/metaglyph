//! General curve–curve intersection (spec plan M4), used by
//! [`crate::skeleton::intersect`] (construction paths, spec §5.9) and
//! [`crate::fill`]'s self-intersection check (spec §8.3). A line-involving
//! pair uses kurbo's own closed-form `PathSeg::intersect_line`; a
//! cubic-against-cubic pair has no such shortcut, so it is solved by
//! recursive bounding-box subdivision (in the same family as the
//! Sederberg–Nishita "Bézier clipping" the spec plan names, though this is
//! the simpler bounding-box variant rather than clipping against a fat
//! line) — halve whichever curve currently has the larger bounding box
//! until both shrink below `tolerance` or a depth cap is hit, then report
//! the midpoint of what remains. A `PathSeg::Quad` never appears in a
//! realized skeleton (`quad` is always degree-elevated to a cubic first,
//! see [`crate::skeleton`]), but is handled here too (by elevation) so
//! this module has no hidden dependency on that invariant.
//!
//! Out of scope: a single cubic self-intersecting against its own interior
//! (a loop from a pathological hand-authored `cube`). Detecting that needs
//! a different test (split the one curve and check the two halves against
//! each other) that this module's pairwise `segment_intersections` does
//! not perform.

use kurbo::{CubicBez, ParamCurve, ParamCurveExtrema, PathSeg, Rect};

/// One crossing between two segments, as each side's own `t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    pub t_a: f64,
    pub t_b: f64,
}

/// The recursion depth cap for [`cubic_cubic`]: halving a curve's
/// parameter range this many times reaches far below any tolerance this
/// module is sensibly called with.
const MAX_DEPTH: u32 = 24;

/// A hard cap on the total number of subdivision calls for one
/// `cubic_cubic` query, bounding the work for pathologically tangent or
/// coincident curves where the bounding boxes keep overlapping — loosely
/// — as they shrink, rather than pruning to a single narrowing branch the
/// way a genuine transversal crossing does.
const MAX_NODES: u32 = 20_000;

/// Every crossing between `a` and `b`, within `tolerance` (a spatial
/// distance in the same units as the curves' coordinates). Order is not
/// meaningful; callers that need one sort by parameter themselves.
pub fn segment_intersections(a: PathSeg, b: PathSeg, tolerance: f64) -> Vec<Crossing> {
    match (a, b) {
        (PathSeg::Line(line), _) => b
            .intersect_line(line)
            .into_iter()
            .map(|hit| Crossing {
                t_a: hit.line_t,
                t_b: hit.segment_t,
            })
            .collect(),
        (_, PathSeg::Line(line)) => a
            .intersect_line(line)
            .into_iter()
            .map(|hit| Crossing {
                t_a: hit.segment_t,
                t_b: hit.line_t,
            })
            .collect(),
        (a, b) => {
            let (ca, cb) = (to_cubic(a), to_cubic(b));
            let mut hits = Vec::new();
            let mut budget = MAX_NODES;
            cubic_cubic(
                ca,
                (0.0, 1.0),
                cb,
                (0.0, 1.0),
                0,
                tolerance,
                &mut budget,
                &mut hits,
            );
            merge_nearby(hits, ca, tolerance)
                .into_iter()
                .map(|(t_a, t_b)| Crossing { t_a, t_b })
                .collect()
        }
    }
}

fn to_cubic(seg: PathSeg) -> CubicBez {
    match seg {
        PathSeg::Cubic(c) => c,
        PathSeg::Quad(q) => q.raise(),
        PathSeg::Line(l) => CubicBez::new(l.p0, l.p0, l.p1, l.p1),
    }
}

fn bbox_diagonal(r: Rect) -> f64 {
    (r.x1 - r.x0).hypot(r.y1 - r.y0)
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

#[allow(clippy::too_many_arguments)]
fn cubic_cubic(
    a: CubicBez,
    a_range: (f64, f64),
    b: CubicBez,
    b_range: (f64, f64),
    depth: u32,
    tolerance: f64,
    budget: &mut u32,
    out: &mut Vec<(f64, f64)>,
) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;

    let box_a = a.bounding_box();
    let box_b = b.bounding_box();
    if !rects_overlap(box_a, box_b) {
        return;
    }

    let converged = bbox_diagonal(box_a).max(bbox_diagonal(box_b)) <= tolerance;
    if depth >= MAX_DEPTH || converged {
        out.push(((a_range.0 + a_range.1) / 2.0, (b_range.0 + b_range.1) / 2.0));
        return;
    }

    if bbox_diagonal(box_a) >= bbox_diagonal(box_b) {
        let mid = (a_range.0 + a_range.1) / 2.0;
        let (a0, a1) = a.subdivide();
        cubic_cubic(
            a0,
            (a_range.0, mid),
            b,
            b_range,
            depth + 1,
            tolerance,
            budget,
            out,
        );
        cubic_cubic(
            a1,
            (mid, a_range.1),
            b,
            b_range,
            depth + 1,
            tolerance,
            budget,
            out,
        );
    } else {
        let mid = (b_range.0 + b_range.1) / 2.0;
        let (b0, b1) = b.subdivide();
        cubic_cubic(
            a,
            a_range,
            b0,
            (b_range.0, mid),
            depth + 1,
            tolerance,
            budget,
            out,
        );
        cubic_cubic(
            a,
            a_range,
            b1,
            (mid, b_range.1),
            depth + 1,
            tolerance,
            budget,
            out,
        );
    }
}

/// Recursive subdivision can converge on the same true crossing from
/// several sibling branches, at slightly different parameters on each
/// side (their local convergence rate depends on the curve's speed
/// there). Merging by evaluated *position* rather than raw parameter
/// values is what actually reflects "the same crossing, found twice."
fn merge_nearby(hits: Vec<(f64, f64)>, a: CubicBez, tolerance: f64) -> Vec<(f64, f64)> {
    // Each leaf converges to within roughly `tolerance` of the true
    // crossing (that's `converged`'s own definition in `cubic_cubic`), so
    // two leaves reporting the same crossing can't be spatially farther
    // apart than a small multiple of it.
    let epsilon = tolerance * 10.0;
    let mut merged: Vec<(f64, f64, kurbo::Point)> = Vec::with_capacity(hits.len());
    for (t_a, t_b) in hits {
        let point = a.eval(t_a);
        let is_duplicate = merged.iter().any(|&(_, _, p)| p.distance(point) < epsilon);
        if !is_duplicate {
            merged.push((t_a, t_b, point));
        }
    }
    merged.into_iter().map(|(t_a, t_b, _)| (t_a, t_b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Line, Point};

    // A realistic scale for this module's actual callers (font-unit
    // curves, typically em ~1000): tight enough to be meaningless to a
    // design, loose enough not to force excessive subdivision in a test.
    const TOLERANCE: f64 = 1e-3;

    #[test]
    fn line_line_matches_kurbo_directly() {
        let a = PathSeg::Line(Line::new((0.0, 0.0), (10.0, 0.0)));
        let b = PathSeg::Line(Line::new((5.0, -5.0), (5.0, 5.0)));
        let hits = segment_intersections(a, b, TOLERANCE);
        assert_eq!(hits.len(), 1);
        assert!((hits[0].t_a - 0.5).abs() < 1e-9);
        assert!((hits[0].t_b - 0.5).abs() < 1e-9);
    }

    #[test]
    fn crossing_cubics_intersect_once_at_the_expected_point() {
        // Two cubics shaped like an X, crossing once near the middle.
        let a = PathSeg::Cubic(CubicBez::new(
            Point::new(0.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(7.0, 10.0),
            Point::new(10.0, 10.0),
        ));
        let b = PathSeg::Cubic(CubicBez::new(
            Point::new(0.0, 10.0),
            Point::new(3.0, 10.0),
            Point::new(7.0, 0.0),
            Point::new(10.0, 0.0),
        ));
        let hits = segment_intersections(a, b, TOLERANCE);
        assert_eq!(hits.len(), 1, "{hits:#?}");
        let pa = to_cubic(a).eval(hits[0].t_a);
        let pb = to_cubic(b).eval(hits[0].t_b);
        let close_enough = TOLERANCE * 10.0;
        assert!((pa.x - pb.x).abs() < close_enough, "{pa:?} vs {pb:?}");
        assert!((pa.y - pb.y).abs() < close_enough, "{pa:?} vs {pb:?}");
        // The crossing is at the shared symmetry point (5, 5).
        assert!((pa.x - 5.0).abs() < 0.5);
        assert!((pa.y - 5.0).abs() < 0.5);
    }

    #[test]
    fn non_overlapping_cubics_have_no_crossing() {
        let a = PathSeg::Cubic(CubicBez::new(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 1.0),
            Point::new(3.0, 0.0),
        ));
        let b = PathSeg::Cubic(CubicBez::new(
            Point::new(0.0, 100.0),
            Point::new(1.0, 101.0),
            Point::new(2.0, 101.0),
            Point::new(3.0, 100.0),
        ));
        assert!(segment_intersections(a, b, TOLERANCE).is_empty());
    }

    #[test]
    fn tangent_curves_terminate_without_exploding() {
        // Identical cubics: bounding boxes always overlap, exercising the
        // depth cap rather than the tolerance-based cutoff.
        let a = PathSeg::Cubic(CubicBez::new(
            Point::new(0.0, 0.0),
            Point::new(3.0, 5.0),
            Point::new(7.0, 5.0),
            Point::new(10.0, 0.0),
        ));
        let hits = segment_intersections(a, a, TOLERANCE);
        assert!(!hits.is_empty());
    }
}
