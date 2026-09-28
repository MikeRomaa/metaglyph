//! Outline preparation (spec §3, §10.1–§10.4): the pure-geometry stages
//! between a glyph's oriented contours and integer `glyf` points, in
//! spec order —
//!
//! 1. [`shear`] every simple outline by the instance slant, and
//!    [`conjugate`] component transforms (spec §10.1, §12.3);
//! 2. [`insert_extrema`] as on-curve points (spec §10.2);
//! 3. [`to_quadratic`] via cu2qu on unrounded cubics (spec §10.3);
//! 4. [`snap_to_zones`] (spec §10.4 step 1);
//! 5. [`quantize`], rounding half away from zero (spec §10.4 step 2);
//! 6. [`drop_zero_length`] segments (spec §10.4 step 3);
//! 7. [`omit_implied`] on-curve points (spec §10.3).
//!
//! Step 7 is spec §10.3's, but runs last: an on-curve point that is the
//! midpoint of its neighbours before rounding need not be after, and
//! omitting it earlier would also keep zone snapping from reaching it.
//! On integers the midpoint test is exact.
//!
//! [`prepare_contour`] runs them all on one contour. The §10.4 step-4
//! re-check of filled contours needs to know which contours are filled
//! and which path they came from, so it lives one level up, in
//! [`crate::prepare`].

use kurbo::{
    Affine, BezPath, CubicBez, ParamCurve, ParamCurveDeriv, ParamCurveExtrema, PathEl, Point,
};
use mg_geom::tolerance::Tolerances;

/// A contour point before quantization: TrueType's representation, where
/// an on-curve point between two consecutive off-curve points is implied
/// (their midpoint) and never stored (spec §10.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplinePoint {
    pub p: Point,
    pub on_curve: bool,
}

/// A quantized contour point, ready for `glyf`. `i32` rather than `i16`:
/// the int16 range is an export check (spec §10.5), not a type invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutlinePoint {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

/// Everything a contour's preparation depends on besides the contour.
#[derive(Debug, Clone)]
pub struct PrepContext {
    /// The instance slant's shear ([`shear`]); the identity when upright.
    pub shear: Affine,
    /// Every metric's `.y` and `.ink`, unrounded (spec §10.4).
    pub zones: Vec<f64>,
    pub tolerances: Tolerances,
}

/// A parameter this close to `0`/`1`, or to a neighbouring extremum, adds
/// no new point worth splitting at.
const EXTREMUM_T_EPSILON: f64 = 1e-6;

/// How far (in design units) an edge's two ends may differ in `y` and
/// still count as horizontal for zone snapping. Extrema insertion makes
/// the tangent handles at a horizontal extremum exact, so this only has
/// to absorb floating-point noise.
const HORIZONTAL_EPSILON: f64 = 1e-6;

/// Spec §12.3's slant shear `(x, y) → (x + y·tan θ, y)`, `slant` = θ in
/// radians.
pub fn shear(slant: f64) -> Affine {
    Affine::new([1.0, 0.0, slant.tan(), 1.0, 0.0, 0.0])
}

/// A component transform `m` under the shear `s`: `S·M·S⁻¹` (spec
/// §10.1), so that the composite of a slanted base glyph equals the
/// slanted decomposed outline.
pub fn conjugate(s: Affine, m: Affine) -> Affine {
    s * m * s.inverse()
}

/// `path` with every cubic split at its horizontal and vertical extrema
/// (spec §10.2). The tangent handles on either side of each new point are
/// set exactly axis-aligned, so later stages can recognise the extremum
/// without a tolerance.
pub fn insert_extrema(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut current = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                out.move_to(p);
                current = p;
            }
            PathEl::LineTo(p) => {
                out.line_to(p);
                current = p;
            }
            PathEl::QuadTo(p1, p2) => {
                let cubic = kurbo::QuadBez::new(current, p1, p2).raise();
                for piece in split_at_extrema(cubic) {
                    out.curve_to(piece.p1, piece.p2, piece.p3);
                }
                current = p2;
            }
            PathEl::CurveTo(p1, p2, p3) => {
                let cubic = CubicBez::new(current, p1, p2, p3);
                for piece in split_at_extrema(cubic) {
                    out.curve_to(piece.p1, piece.p2, piece.p3);
                }
                current = p3;
            }
            PathEl::ClosePath => out.close_path(),
        }
    }
    out
}

fn split_at_extrema(cubic: CubicBez) -> Vec<CubicBez> {
    let mut ts: Vec<f64> = Vec::new();
    for t in cubic.extrema() {
        let clear_of_ends = t > EXTREMUM_T_EPSILON && t < 1.0 - EXTREMUM_T_EPSILON;
        let clear_of_last = ts.last().is_none_or(|&last| t - last > EXTREMUM_T_EPSILON);
        if clear_of_ends && clear_of_last {
            ts.push(t);
        }
    }
    if ts.is_empty() {
        return vec![cubic];
    }

    let deriv = cubic.deriv();
    let mut pieces = Vec::with_capacity(ts.len() + 1);
    let mut t0 = 0.0;
    for &t in ts.iter().chain(std::iter::once(&1.0)) {
        pieces.push(cubic.subsegment(t0..t));
        t0 = t;
    }
    for (k, &t) in ts.iter().enumerate() {
        let at = cubic.eval(t);
        let d = deriv.eval(t);
        let (left, right) = pieces.split_at_mut(k + 1);
        let (left, right) = (&mut left[k], &mut right[0]);
        left.p3 = at;
        right.p0 = at;
        if d.y.abs() <= d.x.abs() {
            // Horizontal tangent: a y-extremum.
            left.p2.y = at.y;
            right.p1.y = at.y;
        } else {
            left.p2.x = at.x;
            right.p1.x = at.x;
        }
    }
    pieces
}

/// Each closed subpath of `path` as a quadratic spline (spec §10.3):
/// lines stay lines, and each cubic becomes kurbo's cu2qu spline within
/// `tolerance`, split in half and retried if no spline of up to kurbo's
/// maximum piece count fits.
pub fn to_quadratic(path: &BezPath, tolerance: f64) -> Vec<Vec<SplinePoint>> {
    let mut contours: Vec<Vec<SplinePoint>> = Vec::new();
    let mut contour: Vec<SplinePoint> = Vec::new();
    let mut current = Point::ZERO;

    let finish = |contour: &mut Vec<SplinePoint>, contours: &mut Vec<Vec<SplinePoint>>| {
        // `glyf` contours close implicitly; an explicit return to the
        // start is a duplicate point.
        if contour.len() > 1 && contour.last().map(|q| q.p) == contour.first().map(|q| q.p) {
            contour.pop();
        }
        if !contour.is_empty() {
            contours.push(std::mem::take(contour));
        }
    };

    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut contour, &mut contours);
                contour.push(on(p));
                current = p;
            }
            PathEl::LineTo(p) => {
                contour.push(on(p));
                current = p;
            }
            PathEl::QuadTo(p1, p2) => {
                contour.push(off(p1));
                contour.push(on(p2));
                current = p2;
            }
            PathEl::CurveTo(p1, p2, p3) => {
                push_cubic(&mut contour, CubicBez::new(current, p1, p2, p3), tolerance);
                current = p3;
            }
            PathEl::ClosePath => finish(&mut contour, &mut contours),
        }
    }
    finish(&mut contour, &mut contours);
    contours
}

fn push_cubic(contour: &mut Vec<SplinePoint>, cubic: CubicBez, tolerance: f64) {
    match cubic.approx_spline(tolerance) {
        Some(spline) => {
            let points = spline.points();
            // `points` is `[p0, off…, p3]`, with the on-curve points
            // between consecutive off-curve ones already implied.
            for &p in &points[1..points.len() - 1] {
                contour.push(off(p));
            }
            contour.push(on(points[points.len() - 1]));
        }
        None => {
            push_cubic(contour, cubic.subsegment(0.0..0.5), tolerance);
            push_cubic(contour, cubic.subsegment(0.5..1.0), tolerance);
        }
    }
}

fn on(p: Point) -> SplinePoint {
    SplinePoint { p, on_curve: true }
}

fn off(p: Point) -> SplinePoint {
    SplinePoint { p, on_curve: false }
}

/// Zone snapping (spec §10.4 step 1): an on-curve point with a horizontal
/// tangent whose `y` lies within `tolerance` of a zone takes that zone,
/// rounded. "Horizontal tangent" means either edge leaving the point is
/// horizontal, which covers smooth extrema and the corners of flat edges
/// alike. An off-curve handle lying on that horizontal moves with its
/// point, so the tangent stays horizontal after the snap. The nearest
/// zone wins; a tie goes to the first in `zones`.
pub fn snap_to_zones(contour: &mut [SplinePoint], zones: &[f64], tolerance: f64) {
    let n = contour.len();
    if n < 2 {
        return;
    }

    let mut moves: Vec<(usize, f64)> = Vec::new();
    for i in 0..n {
        let point = contour[i];
        if !point.on_curve {
            continue;
        }
        let neighbours = [(i + n - 1) % n, (i + 1) % n];
        let horizontal: Vec<usize> = neighbours
            .into_iter()
            .filter(|&j| {
                let q = contour[j].p;
                q != point.p && (q.y - point.p.y).abs() <= HORIZONTAL_EPSILON
            })
            .collect();
        if horizontal.is_empty() {
            continue;
        }

        let mut best: Option<f64> = None;
        for &zone in zones {
            let distance = (point.p.y - zone).abs();
            if distance <= tolerance && best.is_none_or(|b| distance < (point.p.y - b).abs()) {
                best = Some(zone);
            }
        }
        let Some(zone) = best else {
            continue;
        };

        let target = zone.round();
        moves.push((i, target));
        for j in horizontal {
            if !contour[j].on_curve {
                moves.push((j, target));
            }
        }
    }

    for (i, y) in moves {
        contour[i].p.y = y;
    }
}

/// Spec §10.4 step 2: every coordinate to an integer, rounding half away
/// from zero (which is what `f64::round` does).
pub fn quantize(contour: &[SplinePoint]) -> Vec<OutlinePoint> {
    contour
        .iter()
        .map(|q| OutlinePoint {
            x: q.p.x.round() as i32,
            y: q.p.y.round() as i32,
            on_curve: q.on_curve,
        })
        .collect()
}

/// Spec §10.4 step 3: removes segments that rounding made zero-length.
///
/// A run of consecutive points at the same coordinates collapses to one
/// on-curve point there. That is exact for every mix of flags: an
/// off-curve handle coinciding with its on-curve end makes a straight
/// line of the quad, and two coinciding off-curve handles imply an
/// on-curve point at that same spot, with straight lines either side.
/// The contour is cyclic, so the last point is compared with the first.
///
/// A contour left with fewer than three points encloses no area and is
/// dropped entirely (`None`).
pub fn drop_zero_length(contour: &[OutlinePoint]) -> Option<Vec<OutlinePoint>> {
    let same = |a: &OutlinePoint, b: &OutlinePoint| a.x == b.x && a.y == b.y;

    let mut out: Vec<OutlinePoint> = Vec::with_capacity(contour.len());
    for &point in contour {
        match out.last_mut() {
            Some(last) if same(last, &point) => last.on_curve = true,
            _ => out.push(point),
        }
    }
    while out.len() > 1 && same(&out[0], &out[out.len() - 1]) {
        out.pop();
        out[0].on_curve = true;
    }

    (out.len() >= 3).then_some(out)
}

/// Omits every on-curve point that is exactly the midpoint of its two
/// off-curve neighbours (spec §10.3): TrueType implies such a point, so
/// storing it changes nothing but the point count.
pub fn omit_implied(contour: &[OutlinePoint]) -> Vec<OutlinePoint> {
    let n = contour.len();
    let implied = |i: usize| {
        let (prev, here, next) = (contour[(i + n - 1) % n], contour[i], contour[(i + 1) % n]);
        here.on_curve
            && !prev.on_curve
            && !next.on_curve
            && 2 * here.x == prev.x + next.x
            && 2 * here.y == prev.y + next.y
    };
    (0..n)
        .filter(|&i| !implied(i))
        .map(|i| contour[i])
        .collect()
}

/// A quantized contour back as a closed kurbo path, with the implied
/// on-curve points restored — for the self-intersection re-check (spec
/// §10.4 step 4) and for previews.
pub fn to_bezpath(contour: &[OutlinePoint]) -> BezPath {
    let mut path = BezPath::new();
    let n = contour.len();
    if n == 0 {
        return path;
    }
    let pt = |q: &OutlinePoint| Point::new(q.x as f64, q.y as f64);

    // Start on an on-curve point, or on the implied one before the first
    // point if every point is off-curve.
    let start_index = contour.iter().position(|q| q.on_curve);
    let (start, first) = match start_index {
        Some(i) => (pt(&contour[i]), i + 1),
        None => (pt(&contour[n - 1]).midpoint(pt(&contour[0])), 0),
    };
    path.move_to(start);

    let mut pending_off: Option<Point> = None;
    for k in 0..n {
        let index = (first + k) % n;
        if start_index.is_some() && k == n - 1 {
            // The start point itself: close onto it below.
            break;
        }
        let q = &contour[index];
        let p = pt(q);
        match (q.on_curve, pending_off) {
            (true, None) => path.line_to(p),
            (true, Some(c)) => {
                path.quad_to(c, p);
                pending_off = None;
            }
            (false, None) => pending_off = Some(p),
            (false, Some(c)) => {
                path.quad_to(c, c.midpoint(p));
                pending_off = Some(p);
            }
        }
    }
    match pending_off {
        Some(c) => path.quad_to(c, start),
        None => path.line_to(start),
    }
    path.close_path();
    path
}

/// One oriented contour through every stage of this module. Empty when
/// the contour degenerates away under quantization
/// ([`drop_zero_length`]).
pub fn prepare_contour(path: &BezPath, ctx: &PrepContext) -> Vec<Vec<OutlinePoint>> {
    let slanted = ctx.shear * path.clone();
    let with_extrema = insert_extrema(&slanted);
    to_quadratic(&with_extrema, ctx.tolerances.cu2qu)
        .into_iter()
        .filter_map(|mut contour| {
            snap_to_zones(&mut contour, &ctx.zones, ctx.tolerances.zone_snap);
            drop_zero_length(&quantize(&contour)).map(|points| omit_implied(&points))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Shape};

    fn pt(x: i32, y: i32, on_curve: bool) -> OutlinePoint {
        OutlinePoint { x, y, on_curve }
    }

    fn spline(points: &[(f64, f64, bool)]) -> Vec<SplinePoint> {
        points
            .iter()
            .map(|&(x, y, on_curve)| SplinePoint {
                p: Point::new(x, y),
                on_curve,
            })
            .collect()
    }

    #[test]
    fn shear_leans_right_by_tan_theta() {
        let s = shear(10f64.to_radians());
        let p = s * Point::new(5.0, 100.0);
        assert!((p.x - (5.0 + 100.0 * 10f64.to_radians().tan())).abs() < 1e-12);
        assert_eq!(p.y, 100.0);
        assert_eq!(shear(0.0), Affine::IDENTITY);
    }

    #[test]
    fn conjugated_component_equals_slanted_decomposed_outline() {
        let s = shear(12f64.to_radians());
        let m = Affine::translate((120.0, 40.0)) * Affine::scale_non_uniform(-1.0, 0.8);
        let p = Point::new(33.0, 250.0);
        // Composite: the base glyph is slanted (S·p), then placed with
        // the conjugated transform. Decomposed: placed, then slanted.
        let composite = conjugate(s, m) * (s * p);
        let decomposed = s * (m * p);
        assert!((composite - decomposed).hypot() < 1e-9);
    }

    #[test]
    fn extrema_split_a_circle_with_exactly_axis_aligned_handles() {
        // A circle rotated 45° so kurbo's own quadrant joints are not at
        // the extrema.
        let circle = Affine::rotate(std::f64::consts::FRAC_PI_4)
            * Circle::new((0.0, 0.0), 100.0).to_path(1e-3);
        let split = insert_extrema(&circle);

        let mut into_top = 0;
        let mut out_of_right = 0;
        for seg in split.segments() {
            let kurbo::PathSeg::Cubic(c) = seg else {
                continue;
            };
            if (c.p3.y - 100.0).abs() < 0.1 {
                assert_eq!(c.p2.y, c.p3.y, "the handle into the top is horizontal");
                into_top += 1;
            }
            if (c.p0.x - 100.0).abs() < 0.1 {
                assert_eq!(c.p1.x, c.p0.x, "the handle out of the right is vertical");
                out_of_right += 1;
            }
        }
        assert_eq!(into_top, 1);
        assert_eq!(out_of_right, 1);
        // Splitting leaves the shape alone.
        assert!((split.area() - circle.area()).abs() < 1e-6);
    }

    #[test]
    fn an_already_monotone_cubic_is_not_split() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.curve_to((30.0, 0.0), (100.0, 70.0), (100.0, 100.0));
        assert_eq!(insert_extrema(&path).elements().len(), 2);
    }

    #[test]
    fn cu2qu_stays_within_tolerance() {
        let circle = Circle::new((0.0, 0.0), 300.0).to_path(1e-3);
        let tolerance = 0.5;
        let contours = to_quadratic(&insert_extrema(&circle), tolerance);
        assert_eq!(contours.len(), 1);
        let contour = &contours[0];

        // Every point of the quadratic outline is within tolerance of the
        // circle. Scaled by 1000 on the way through `to_bezpath` so its
        // integer rounding is negligible.
        let fine: Vec<OutlinePoint> = contour
            .iter()
            .map(|q| OutlinePoint {
                x: (q.p.x * 1000.0).round() as i32,
                y: (q.p.y * 1000.0).round() as i32,
                on_curve: q.on_curve,
            })
            .collect();
        let path = Affine::scale(1e-3) * to_bezpath(&fine);
        for seg in path.segments() {
            for k in 0..=16 {
                let r = seg.eval(k as f64 / 16.0).to_vec2().hypot();
                assert!((r - 300.0).abs() <= tolerance + 0.01, "r = {r}");
            }
        }
    }

    #[test]
    fn a_line_only_contour_passes_through_cu2qu_unchanged() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        path.line_to((10.0, 10.0));
        path.line_to((0.0, 0.0));
        path.close_path();
        assert_eq!(
            to_quadratic(&path, 0.5),
            vec![spline(&[
                (0.0, 0.0, true),
                (10.0, 0.0, true),
                (10.0, 10.0, true)
            ])]
        );
    }

    #[test]
    fn a_horizontal_extremum_snaps_to_a_nearby_zone_with_its_handles() {
        // The top of a bowl at y = 711.4, near capHeight's ink (712).
        let mut contour = spline(&[
            (0.0, 0.0, true),
            (0.0, 711.4, false),
            (100.0, 711.4, true),
            (200.0, 711.4, false),
            (200.0, 0.0, true),
        ]);
        snap_to_zones(&mut contour, &[700.0, 712.0], 1.0);
        assert_eq!(contour[1].p.y, 712.0);
        assert_eq!(contour[2].p.y, 712.0);
        assert_eq!(contour[3].p.y, 712.0);
        assert_eq!(contour[0].p.y, 0.0);
    }

    #[test]
    fn a_point_without_a_horizontal_tangent_does_not_snap() {
        let mut contour = spline(&[(0.0, 0.0, true), (50.0, 699.6, true), (100.0, 0.0, true)]);
        snap_to_zones(&mut contour, &[700.0], 1.0);
        assert_eq!(contour[1].p.y, 699.6);
    }

    #[test]
    fn a_point_beyond_the_tolerance_does_not_snap() {
        let mut contour = spline(&[(0.0, 697.0, true), (100.0, 697.0, true), (50.0, 0.0, true)]);
        snap_to_zones(&mut contour, &[700.0], 1.0);
        assert_eq!(contour[0].p.y, 697.0);
    }

    #[test]
    fn the_nearest_zone_wins_and_snaps_to_its_rounded_value() {
        let mut contour = spline(&[(0.0, 10.2, true), (100.0, 10.2, true), (50.0, 300.0, true)]);
        snap_to_zones(&mut contour, &[9.4, 10.6], 1.0);
        assert_eq!(contour[0].p.y, 11.0);
        assert_eq!(contour[1].p.y, 11.0);
    }

    #[test]
    fn quantize_rounds_half_away_from_zero() {
        let q = quantize(&spline(&[
            (0.5, -0.5, true),
            (2.5, -2.5, false),
            (1.49, -1.51, true),
        ]));
        assert_eq!(q, vec![pt(1, -1, true), pt(3, -3, false), pt(1, -2, true)]);
    }

    #[test]
    fn coincident_points_collapse_to_one_on_curve_point() {
        let contour = [
            pt(0, 0, true),
            pt(5, 0, false),
            pt(5, 0, false), // two coincident handles: an on-curve point
            pt(10, 0, true),
            pt(10, 10, false),
            pt(10, 10, true), // a handle on its own end: a line
            pt(0, 10, true),
            pt(0, 0, true), // back on the start
        ];
        assert_eq!(
            drop_zero_length(&contour),
            Some(vec![
                pt(0, 0, true),
                pt(5, 0, true),
                pt(10, 0, true),
                pt(10, 10, true),
                pt(0, 10, true),
            ])
        );
    }

    #[test]
    fn a_contour_rounded_down_to_a_point_is_dropped() {
        let contour = [pt(3, 3, true), pt(3, 3, false), pt(3, 4, true)];
        assert_eq!(drop_zero_length(&contour), None);
    }

    #[test]
    fn exact_midpoints_between_handles_are_omitted() {
        let contour = [
            pt(0, 0, true),
            pt(0, 10, false),
            pt(5, 10, true), // exact midpoint: omitted
            pt(10, 10, false),
            pt(11, 5, true), // between an off and an on point: kept
            pt(10, 0, false),
            pt(6, 0, true), // not the midpoint of its handles: kept
            pt(1, 0, false),
        ];
        assert_eq!(
            omit_implied(&contour),
            vec![
                pt(0, 0, true),
                pt(0, 10, false),
                pt(10, 10, false),
                pt(11, 5, true),
                pt(10, 0, false),
                pt(6, 0, true),
                pt(1, 0, false),
            ]
        );
    }

    #[test]
    fn to_bezpath_restores_implied_on_curve_points() {
        let contour = [
            pt(0, 0, true),
            pt(0, 10, false),
            pt(10, 10, false),
            pt(10, 0, true),
        ];
        let path = to_bezpath(&contour);
        let els = path.elements();
        assert_eq!(els[0], PathEl::MoveTo(Point::new(0.0, 0.0)));
        assert_eq!(
            els[1],
            PathEl::QuadTo(Point::new(0.0, 10.0), Point::new(5.0, 10.0))
        );
        assert_eq!(
            els[2],
            PathEl::QuadTo(Point::new(10.0, 10.0), Point::new(10.0, 0.0))
        );
        assert_eq!(els[3], PathEl::LineTo(Point::new(0.0, 0.0)));
    }

    #[test]
    fn to_bezpath_handles_an_all_off_curve_contour() {
        let contour = [
            pt(0, 10, false),
            pt(10, 10, false),
            pt(10, 0, false),
            pt(0, 0, false),
        ];
        let path = to_bezpath(&contour);
        assert_eq!(path.elements()[0], PathEl::MoveTo(Point::new(0.0, 5.0)));
        assert_eq!(path.segments().count(), 4);
        // The 50-unit diamond plus four parabolic segments, each 2/3 of
        // its 12.5-unit corner triangle.
        assert!((path.area().abs() - 250.0 / 3.0).abs() < 1e-9);
    }
}
