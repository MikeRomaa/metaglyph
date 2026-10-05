//! Fold trimming (spec §7.2, §15.5): where the skeleton curves tighter
//! than the stroke radius, the trimmed outline must cover exactly the ink
//! of a round pen swept along the skeleton. Each case rasterizes the
//! outline (nonzero, all contours together) and compares it with the
//! pen's own coverage — every point within `r` of the skeleton — away
//! from a thin band along the true boundary.

use kurbo::{BezPath, ParamCurveNearest, PathSeg, Point, Shape};
use mg_geom::skeleton::{self, ArcGeometry, RawSegment, RawStart, Skeleton, Sweep};
use mg_geom::stroke::{Cap, JoinKind, StrokeSpec, Stroked, stroke_path_with_folds};
use mg_geom::winding::ContourRole;

const TOLERANCE: f64 = 0.05;
/// Points this close to the pen's boundary are not compared: the outline
/// is only within `TOLERANCE` of it, and a crossing cut sits on it.
const BAND: f64 = 0.5;

fn spec(width: f64) -> StrokeSpec {
    StrokeSpec {
        width,
        start_cap: Cap::Round,
        end_cap: Cap::Round,
        default_join: JoinKind::Round,
        join_overrides: Vec::new(),
    }
}

fn arc(to: Point, rx: f64, ry: f64, large: bool, sweep: Sweep) -> RawSegment {
    RawSegment::Arc {
        to,
        geometry: ArcGeometry::Radii { rx, ry, large },
        sweep,
    }
}

/// The point at `deg` on the ellipse about the origin with radii `rx`, `ry`.
fn on(rx: f64, ry: f64, deg: f64) -> Point {
    Point::new(rx * deg.to_radians().cos(), ry * deg.to_radians().sin())
}

/// Asserts `stroked` covers what a pen of radius `r` swept along
/// `skeleton` covers, on a grid over the pen's reach.
fn assert_pen_coverage(skeleton: &Skeleton, stroked: &Stroked, r: f64) {
    let wrong = pen_mismatches(skeleton, stroked, r);
    assert!(
        wrong.is_empty(),
        "{} points disagree with the pen, e.g. {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(5)]
    );
}

/// Grid points where `stroked`'s ink differs from the pen's: `(point,
/// distance to the skeleton, inked)`.
fn pen_mismatches(skeleton: &Skeleton, stroked: &Stroked, r: f64) -> Vec<(Point, f64, bool)> {
    let pieces: Vec<PathSeg> = skeleton.path.segments().collect();
    // Flattened: kurbo's `BezPath::winding` misjudges some points just
    // outside a cubic.
    let outline: Vec<BezPath> = stroked
        .contours
        .iter()
        .map(|(c, _)| {
            let mut polygon = BezPath::new();
            kurbo::flatten(c.elements().iter().copied(), TOLERANCE / 10.0, |el| {
                polygon.push(el)
            });
            polygon
        })
        .collect();
    let bbox = skeleton.path.bounding_box().inflate(r + 1.0, r + 1.0);
    let steps = 200;
    let mut wrong = Vec::new();
    for i in 0..steps {
        for j in 0..steps {
            let p = Point::new(
                bbox.x0 + bbox.width() * (i as f64 + 0.5) / steps as f64,
                bbox.y0 + bbox.height() * (j as f64 + 0.5) / steps as f64,
            );
            let d = pieces
                .iter()
                .map(|s| s.nearest(p, 1e-9).distance_sq)
                .fold(f64::INFINITY, f64::min)
                .sqrt();
            if (d - r).abs() < BAND {
                continue;
            }
            let inked = outline.iter().map(|c| c.winding(p)).sum::<i32>() != 0;
            if inked != (d < r) {
                wrong.push((p, d, inked));
            }
        }
    }
    wrong
}

#[test]
fn a_narrow_ellipse_vertex_comes_to_a_point() {
    // The `!` in samples/a22x-mono.mg: rx 60, ry 120, curvature radius 30
    // at the top, stroked at 100 (r = 50).
    let (rx, ry) = (60.0, 120.0);
    let skeleton = skeleton::realize(
        &RawStart { at: on(rx, ry, 185.0) },
        &[arc(on(rx, ry, -5.0), rx, ry, true, Sweep::Cw)],
        false,
        1e-9,
    )
    .unwrap();
    let stroked = stroke_path_with_folds(&skeleton, false, &spec(100.0), TOLERANCE).unwrap();
    assert_eq!(stroked.folds.len(), 1, "{:?}", stroked.folds);
    assert_eq!(stroked.contours.len(), 1);
    assert_pen_coverage(&skeleton, &stroked, 50.0);

    // Untrimmed, kurbo's outline crosses itself on the inner side there
    // (rendering the same ink under nonzero, but as a looped outline);
    // trimmed, it doesn't.
    let raw = kurbo::stroke(
        skeleton.path.elements().iter().copied(),
        &kurbo::Stroke::new(100.0).with_caps(kurbo::Cap::Round),
        &kurbo::StrokeOpts::default(),
        TOLERANCE,
    );
    assert!(self_crossings(&raw) > 0);
    assert_eq!(self_crossings(&stroked.contours[0].0), 0);
}

/// Crossings between non-neighbouring pieces of one closed contour, away
/// from the pieces' own joints (kurbo can leave a tiny piece at a joint,
/// so its neighbours meet across it).
fn self_crossings(contour: &BezPath) -> usize {
    use kurbo::ParamCurve;
    let segs: Vec<PathSeg> = contour.segments().collect();
    let n = segs.len();
    let mut count = 0;
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let ends = [segs[i].start(), segs[i].end(), segs[j].start(), segs[j].end()];
            count += mg_geom::intersect::segment_intersections(segs[i], segs[j], 1e-6)
                .iter()
                .filter(|c| {
                    let p = segs[i].eval(c.t_a.clamp(0.0, 1.0));
                    ends.iter().all(|e| e.distance(p) > 4.0 * TOLERANCE)
                })
                .count();
        }
    }
    count
}

#[test]
fn a_closed_ellipse_keeps_its_counter_between_two_folds() {
    // Folds at the top and bottom; the counter survives as a lens.
    let (rx, ry) = (60.0, 120.0);
    let skeleton = skeleton::realize(
        &RawStart { at: on(rx, ry, 0.0) },
        &[
            arc(on(rx, ry, 180.0), rx, ry, false, Sweep::Ccw),
            arc(on(rx, ry, 0.0), rx, ry, false, Sweep::Ccw),
        ],
        true,
        1e-9,
    )
    .unwrap();
    let stroked = stroke_path_with_folds(&skeleton, true, &spec(100.0), TOLERANCE).unwrap();
    assert_eq!(stroked.folds.len(), 2, "{:?}", stroked.folds);
    let roles: Vec<ContourRole> = stroked.contours.iter().map(|(_, r)| *r).collect();
    assert_eq!(roles.len(), 2, "{roles:?}");
    assert!(roles.contains(&ContourRole::Counter), "{roles:?}");
    assert_pen_coverage(&skeleton, &stroked, 50.0);
    for (contour, _) in &stroked.contours {
        assert_eq!(self_crossings(contour), 0);
    }
}

#[test]
fn a_closed_ellipse_whose_counter_closes_up() {
    // r = 70 is wider than the ellipse's half-width (60): no point inside
    // is farther than r from the skeleton, so there is no counter.
    let (rx, ry) = (60.0, 120.0);
    let skeleton = skeleton::realize(
        &RawStart { at: on(rx, ry, 0.0) },
        &[
            arc(on(rx, ry, 180.0), rx, ry, false, Sweep::Ccw),
            arc(on(rx, ry, 0.0), rx, ry, false, Sweep::Ccw),
        ],
        true,
        1e-9,
    )
    .unwrap();
    let stroked = stroke_path_with_folds(&skeleton, true, &spec(140.0), TOLERANCE).unwrap();
    assert_eq!(stroked.contours.len(), 1, "{:?}", stroked.contours.len());
    assert_eq!(stroked.contours[0].1, ContourRole::Outer);
    assert_pen_coverage(&skeleton, &stroked, 70.0);
}

#[test]
fn a_fold_running_into_a_round_cap() {
    // A short, tight arc: the fold's loop reaches past the arc's end,
    // into the round caps.
    let skeleton = skeleton::realize(
        &RawStart { at: on(20.0, 40.0, 120.0) },
        &[arc(on(20.0, 40.0, 60.0), 20.0, 40.0, false, Sweep::Cw)],
        false,
        1e-9,
    )
    .unwrap();
    let stroked = stroke_path_with_folds(&skeleton, false, &spec(60.0), TOLERANCE).unwrap();
    assert!(!stroked.folds.is_empty());
    assert_pen_coverage(&skeleton, &stroked, 30.0);
    assert_eq!(self_crossings(&stroked.contours[0].0), 0);
}

#[test]
fn a_wide_curve_is_untouched() {
    let skeleton = skeleton::realize(
        &RawStart { at: on(60.0, 120.0, 185.0) },
        &[arc(on(60.0, 120.0, -5.0), 60.0, 120.0, true, Sweep::Cw)],
        false,
        1e-9,
    )
    .unwrap();
    let stroked = stroke_path_with_folds(&skeleton, false, &spec(40.0), TOLERANCE).unwrap();
    assert!(stroked.folds.is_empty());
    assert_pen_coverage(&skeleton, &stroked, 20.0);
}
