//! The construction library (spec §5.9): every callable name, as a real
//! computation over already-evaluated [`Value`]s. `mg-hir` (M2) already
//! statically checked each call's arity and argument types against this
//! same signature table (`mg_hir::types::lookup_function`), so a shape
//! mismatch here — wrong count, wrong `Value` variant — is a bug
//! upstream, not a user-facing error; only genuine spec §13 domain
//! errors are [`EvalError`]s.

use kurbo::{Affine, Line, ParamCurve, ParamCurveArclen, Point, Vec2};
use mg_geom::skeleton;

use crate::errors::EvalError;
use crate::value::{Ellipse, Rect, Value};

/// Arc-length and curve–curve intersection numerics have no spec-named
/// tolerance (unlike the M4 constants in spec §14); this is just "close
/// enough" for `mg dump-graph` and this crate's own tests.
const ARC_ACCURACY: f64 = 1e-6;

fn num(v: &Value) -> f64 {
    v.as_num()
        .expect("mg-hir already type-checked this argument as `num`")
}

fn pair(v: &Value) -> Point {
    v.as_pair()
        .expect("mg-hir already type-checked this argument as `pair`")
}

fn line(v: &Value) -> Line {
    v.as_line()
        .expect("mg-hir already type-checked this argument as `line`")
}

fn ellipse(v: &Value) -> Ellipse {
    v.as_ellipse()
        .expect("mg-hir already type-checked this argument as `ellipse`")
}

fn transform(v: &Value) -> Affine {
    v.as_transform()
        .expect("mg-hir already type-checked this argument as `transform`")
}

fn path(v: &Value) -> &mg_geom::skeleton::Skeleton {
    v.as_path()
        .expect("mg-hir already type-checked this argument as `path`")
}

fn num_list(v: &Value) -> Vec<f64> {
    v.as_num_list()
        .expect("mg-hir already type-checked this argument as `list<num>`")
}

/// Dispatches one call (spec §5.9). `name` is assumed to be a real
/// function name — callers resolve that separately, the same way
/// `mg_hir::type_check` does — so an unknown name here is also a bug
/// upstream, not a domain error. `arc_tolerance` is spec §14's
/// `ARC_TOLERANCE`, for the line–ellipse queries.
pub fn call(name: &str, args: &[Value], arc_tolerance: f64) -> Result<Value, EvalError> {
    match name {
        "abs" => Ok(Value::Num(num(&args[0]).abs())),
        "sign" => Ok(Value::Num(num(&args[0]).signum())),
        "floor" => Ok(Value::Num(num(&args[0]).floor())),
        "ceil" => Ok(Value::Num(num(&args[0]).ceil())),
        "round" => Ok(Value::Num(round_half_away_from_zero(num(&args[0])))),
        "sqrt" => {
            let x = num(&args[0]);
            if x < 0.0 {
                Err(EvalError::SqrtOfNegative(x))
            } else {
                Ok(Value::Num(x.sqrt()))
            }
        }
        "exp" => Ok(Value::Num(num(&args[0]).exp())),
        "log" => {
            let x = num(&args[0]);
            if x <= 0.0 {
                Err(EvalError::LogOfNonPositive(x))
            } else {
                Ok(Value::Num(x.ln()))
            }
        }
        "sin" => Ok(Value::Num(num(&args[0]).sin())),
        "cos" => Ok(Value::Num(num(&args[0]).cos())),
        "tan" => Ok(Value::Num(num(&args[0]).tan())),
        "asin" => inverse_trig("asin", num(&args[0]), f64::asin),
        "acos" => inverse_trig("acos", num(&args[0]), f64::acos),
        "atan2" => Ok(Value::Num(num(&args[0]).atan2(num(&args[1])))),
        "min" => Ok(Value::Num(num(&args[0]).min(num(&args[1])))),
        "max" => Ok(Value::Num(num(&args[0]).max(num(&args[1])))),
        "clamp" => {
            let (x, lo, hi) = (num(&args[0]), num(&args[1]), num(&args[2]));
            Ok(Value::Num(x.clamp(lo, hi)))
        }
        "lerp" => {
            let (a, b, t) = (num(&args[0]), num(&args[1]), num(&args[2]));
            Ok(Value::Num(a + (b - a) * t))
        }

        "length" => Ok(Value::Num(pair(&args[0]).to_vec2().length())),
        "angle" => Ok(Value::Num(pair(&args[0]).to_vec2().angle())),
        "unit" => {
            let v = pair(&args[0]).to_vec2();
            if v == Vec2::ZERO {
                Err(EvalError::UnitOfZeroVector)
            } else {
                Ok(Value::Pair(v.normalize().to_point()))
            }
        }
        "dir" => Ok(Value::Pair(Vec2::from_angle(num(&args[0])).to_point())),
        "dot" => Ok(Value::Num(
            pair(&args[0]).to_vec2().dot(pair(&args[1]).to_vec2()),
        )),
        "cross" => Ok(Value::Num(
            pair(&args[0]).to_vec2().cross(pair(&args[1]).to_vec2()),
        )),
        "perpendicular" => {
            let v = pair(&args[0]).to_vec2();
            Ok(Value::Pair(Vec2::new(-v.y, v.x).to_point()))
        }

        "meet" => line(&args[0])
            .crossing_point(line(&args[1]))
            .map(Value::Pair)
            .ok_or(EvalError::MeetOnParallelLines),
        "mediate" => {
            let (a, b, t) = (pair(&args[0]), pair(&args[1]), num(&args[2]));
            Ok(Value::Pair(a.lerp(b, t)))
        }
        "project" => Ok(Value::Pair(project_onto_line(
            pair(&args[0]),
            line(&args[1]),
        ))),
        "polar" => {
            let (p, len, theta) = (pair(&args[0]), num(&args[1]), num(&args[2]));
            Ok(Value::Pair(p + Vec2::from_angle(theta) * len))
        }
        "mirror" => {
            let l = line(&args[1]);
            let reflect = Affine::reflect(l.p0, l.p1 - l.p0);
            Ok(Value::Pair(reflect * pair(&args[0])))
        }

        "lineThrough" => {
            let (a, b) = (pair(&args[0]), pair(&args[1]));
            if a == b {
                Err(EvalError::LineThroughOnePoint)
            } else {
                Ok(Value::Line(Line::new(a, b)))
            }
        }
        "lineAt" => {
            let (p, theta) = (pair(&args[0]), num(&args[1]));
            Ok(Value::Line(Line::new(p, p + Vec2::from_angle(theta))))
        }
        "hline" => {
            let y = num(&args[0]);
            Ok(Value::Line(Line::new((0.0, y), (1.0, y))))
        }
        "vline" => {
            let x = num(&args[0]);
            Ok(Value::Line(Line::new((x, 0.0), (x, 1.0))))
        }

        "ellipse" | "circle" => {
            let center = pair(&args[0]);
            let rx = num(&args[1]);
            let ry = if name == "circle" { rx } else { num(&args[2]) };
            for r in [rx, ry] {
                if r <= 0.0 {
                    return Err(EvalError::NonPositiveRadius(r));
                }
            }
            Ok(Value::Ellipse(Ellipse { center, rx, ry }))
        }
        "crossings" => Ok(Value::List(
            crossings(line(&args[0]), ellipse(&args[1]), arc_tolerance)
                .into_iter()
                .map(Value::Num)
                .collect(),
        )),
        "along" => {
            let (origin, direction) = ray(line(&args[0]));
            Ok(Value::Pair(origin + direction * num(&args[1])))
        }
        "cast" => {
            let l = line(&args[0]);
            let (origin, direction) = ray(l);
            crossings(l, ellipse(&args[1]), arc_tolerance)
                .into_iter()
                .find(|&s| s > arc_tolerance)
                .map(|s| Value::Pair(origin + direction * s))
                .ok_or(EvalError::CastMissesEllipse)
        }

        "translate" => Ok(Value::Transform(Affine::translate((
            num(&args[0]),
            num(&args[1]),
        )))),
        "rotate" => Ok(Value::Transform(Affine::rotate(num(&args[0])))),
        "scale" if args.len() == 1 => Ok(Value::Transform(Affine::scale(num(&args[0])))),
        "scale" => Ok(Value::Transform(Affine::scale_non_uniform(
            num(&args[0]),
            num(&args[1]),
        ))),
        "slant" => {
            let theta = num(&args[0]);
            Ok(Value::Transform(Affine::new([
                1.0,
                0.0,
                theta.tan(),
                1.0,
                0.0,
                0.0,
            ])))
        }
        "reflect" => {
            let l = line(&args[0]);
            Ok(Value::Transform(Affine::reflect(l.p0, l.p1 - l.p0)))
        }
        "apply" => Ok(Value::Pair(transform(&args[0]) * pair(&args[1]))),

        "pointAt" => {
            let (seg, t) = resolve_param(path(&args[0]), num(&args[1]))?;
            Ok(Value::Pair(seg.eval(t)))
        }
        "directionAt" => {
            let (seg, t) = resolve_param(path(&args[0]), num(&args[1]))?;
            let dir = skeleton::direction_at(&seg, t);
            if dir == Vec2::ZERO {
                Err(EvalError::UnitOfZeroVector)
            } else {
                Ok(Value::Pair(dir.normalize().to_point()))
            }
        }
        "curvatureAt" => {
            let (seg, t) = resolve_param(path(&args[0]), num(&args[1]))?;
            Ok(Value::Num(skeleton::curvature_at(&seg, t)))
        }
        "arcLength" => Ok(Value::Num(skeleton::arc_length(
            &path(&args[0]).path,
            ARC_ACCURACY,
        ))),
        "pointAtLength" => Ok(Value::Pair(point_at_length(path(&args[0]), num(&args[1])))),
        "intersect" => {
            let hits = skeleton::intersect(path(&args[0]), &path(&args[1]).path, ARC_ACCURACY);
            Ok(Value::List(hits.into_iter().map(Value::Num).collect()))
        }
        "subpath" => {
            let (t0, t1) = (num(&args[1]), num(&args[2]));
            Ok(Value::Path(subpath(path(&args[0]), t0, t1)?))
        }
        "reverse" => {
            let reversed = path(&args[0]).path.reverse_subpaths();
            Ok(Value::Path(mg_geom::skeleton::Skeleton::from_path(
                reversed,
            )))
        }
        "extrema" => Ok(Value::List(
            skeleton::extrema(path(&args[0]))
                .into_iter()
                .map(Value::Num)
                .collect(),
        )),

        "sum" => Ok(Value::Num(num_list(&args[0]).into_iter().sum())),
        "minOf" => reduce(&args[0], "minOf", f64::min),
        "maxOf" => reduce(&args[0], "maxOf", f64::max),

        _ => unreachable!("mg-hir already resolved `{name}` against the function table"),
    }
}

/// `spec §5.8`'s `^`, evaluated for real: `0^0 = 1`, and a negative base
/// with a non-integer exponent is a domain error.
pub fn power(base: f64, exponent: f64) -> Result<f64, EvalError> {
    if base == 0.0 && exponent == 0.0 {
        return Ok(1.0);
    }
    if base < 0.0 && exponent.fract() != 0.0 {
        return Err(EvalError::PowerDomainError { base, exponent });
    }
    Ok(base.powf(exponent))
}

pub fn division(numerator: f64, denominator: f64) -> Result<f64, EvalError> {
    if denominator == 0.0 {
        Err(EvalError::DivisionByZero)
    } else {
        Ok(numerator / denominator)
    }
}

fn round_half_away_from_zero(x: f64) -> f64 {
    if x >= 0.0 {
        (x + 0.5).floor()
    } else {
        (x - 0.5).ceil()
    }
}

fn inverse_trig(function: &'static str, value: f64, f: fn(f64) -> f64) -> Result<Value, EvalError> {
    if !(-1.0..=1.0).contains(&value) {
        return Err(EvalError::InverseTrigOutOfRange { function, value });
    }
    Ok(Value::Num(f(value)))
}

/// The perpendicular foot of `p` on the infinite line through `line`
/// (spec §5.9 `project`) — `Line`'s own `ParamCurveNearest` clamps to the
/// segment `[0, 1]`, which is the wrong shape for an infinite line, so
/// this projects by hand instead.
/// A line's origin and unit direction (spec §5.9). Every constructor
/// gives `p1 ≠ p0`, so the direction is defined.
fn ray(line: Line) -> (Point, Vec2) {
    (line.p0, (line.p1 - line.p0).normalize())
}

/// Distances along `line` from its origin where it crosses `e`, ascending
/// (spec §5.9): two, one when tangent within `tolerance`, or none.
///
/// Solved in coordinates scaled by `(1/rx, 1/ry)`, where `e` is the unit
/// circle: `|u + s·v|² = 1`. Tangency measures the line's nearest approach
/// to the centre (in scaled terms) against the ellipse point on the same
/// ray from the centre.
fn crossings(line: Line, e: Ellipse, tolerance: f64) -> Vec<f64> {
    let (origin, direction) = ray(line);
    let scale = |v: Vec2| Vec2::new(v.x / e.rx, v.y / e.ry);
    let u = scale(origin - e.center);
    let v = scale(direction);
    let a = v.hypot2();
    let nearest = -u.dot(v) / a;
    // The nearest approach's scaled radius: 1 on the ellipse.
    let m = (u + v * nearest).hypot();
    if m > 0.0 {
        // Its distance from the ellipse, along the ray from the centre.
        let r = (origin + direction * nearest - e.center).hypot();
        if r * (1.0 - 1.0 / m).abs() <= tolerance {
            return vec![nearest];
        }
    }
    if m > 1.0 {
        return Vec::new();
    }
    let half = (1.0 - m * m).sqrt() / a.sqrt();
    vec![nearest - half, nearest + half]
}

fn project_onto_line(p: Point, line: Line) -> Point {
    let dir = line.p1 - line.p0;
    let t = (p - line.p0).dot(dir) / dir.dot(dir);
    line.p0 + dir * t
}

fn reduce(
    list: &Value,
    function: &'static str,
    f: fn(f64, f64) -> f64,
) -> Result<Value, EvalError> {
    let values = num_list(list);
    values
        .into_iter()
        .reduce(f)
        .map(Value::Num)
        .ok_or(EvalError::EmptyListReduction { function })
}

/// `t`'s segment and local parameter (spec §5.9 domain, over authored
/// segments — see `mg_geom::skeleton::resolve_param`).
fn resolve_param(
    skeleton: &mg_geom::skeleton::Skeleton,
    t: f64,
) -> Result<(kurbo::PathSeg, f64), EvalError> {
    skeleton::resolve_param(skeleton, t).map_err(|_| EvalError::PathParameterOutOfDomain {
        param: t,
        max: skeleton.piece_counts.len() as f64,
    })
}

fn point_at_length(skeleton: &mg_geom::skeleton::Skeleton, s: f64) -> Point {
    let mut remaining = s;
    let segs = skeleton::segments(&skeleton.path);
    for seg in &segs {
        let len = seg.arclen(ARC_ACCURACY);
        if remaining <= len || std::ptr::eq(seg, segs.last().unwrap()) {
            let t = seg.inv_arclen(remaining.max(0.0), ARC_ACCURACY);
            return seg.eval(t);
        }
        remaining -= len;
    }
    skeleton
        .path
        .segments()
        .last()
        .map(|s| s.eval(1.0))
        .unwrap_or_default()
}

/// `subpath`'s result is a construction value with no `arc`s of its own
/// to track (spec §5.9: "it can never render"),
/// so it comes back as one piece per authored segment.
fn subpath(
    skeleton: &mg_geom::skeleton::Skeleton,
    t0: f64,
    t1: f64,
) -> Result<mg_geom::skeleton::Skeleton, EvalError> {
    let out_of_domain = || EvalError::PathParameterOutOfDomain {
        param: t0.max(t1),
        max: skeleton.piece_counts.len() as f64,
    };
    let (piece0, start_t) =
        mg_geom::skeleton::authored_param_to_piece(skeleton, t0).map_err(|_| out_of_domain())?;
    let (piece1, end_t) =
        mg_geom::skeleton::authored_param_to_piece(skeleton, t1).map_err(|_| out_of_domain())?;

    let pieces = skeleton::segments(&skeleton.path);
    let mut out = kurbo::BezPath::new();
    out.move_to(pieces[piece0].eval(start_t));

    for (i, seg) in pieces.iter().enumerate().take(piece1 + 1).skip(piece0) {
        let lo = if i == piece0 { start_t } else { 0.0 };
        let hi = if i == piece1 { end_t } else { 1.0 };
        match seg.subsegment(lo..hi) {
            kurbo::PathSeg::Line(l) => out.line_to(l.p1),
            kurbo::PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            kurbo::PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
    }
    Ok(mg_geom::skeleton::Skeleton::from_path(out))
}

/// The tight skeleton bounds (spec §5.5 `path.bbox` for a construction
/// path; a rendering path needs the stroked/filled outline instead —
/// `mg-eval`'s glyph-level evaluation branches on that before ever
/// calling this).
pub fn bbox(bez: &kurbo::BezPath) -> Rect {
    skeleton::bounding_box(bez).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tolerance the tests below run at: `ARC_TOLERANCE` at em 1000.
    const TOL: f64 = 0.01;

    fn call(name: &str, args: &[Value]) -> Result<Value, EvalError> {
        super::call(name, args, TOL)
    }

    fn p(x: f64, y: f64) -> Value {
        Value::Pair(Point::new(x, y))
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn approx_point(v: &Value, x: f64, y: f64) {
        let got = v.as_pair().unwrap();
        approx(got.x, x);
        approx(got.y, y);
    }

    #[test]
    fn meet_of_two_lines() {
        let a = Value::Line(Line::new((0.0, 0.0), (0.0, 1.0))); // vertical, x = 0
        let b = Value::Line(Line::new((0.0, 5.0), (1.0, 5.0))); // horizontal, y = 5
        let result = call("meet", &[a, b]).unwrap();
        approx_point(&result, 0.0, 5.0);
    }

    #[test]
    fn meet_of_parallel_lines_is_a_domain_error() {
        let a = Value::Line(Line::new((0.0, 0.0), (1.0, 0.0)));
        let b = Value::Line(Line::new((0.0, 1.0), (1.0, 1.0)));
        assert_eq!(call("meet", &[a, b]), Err(EvalError::MeetOnParallelLines));
    }

    #[test]
    fn mediate_is_a_linear_interpolation() {
        let result = call("mediate", &[p(0.0, 0.0), p(10.0, 0.0), Value::Num(0.25)]).unwrap();
        approx_point(&result, 2.5, 0.0);
    }

    #[test]
    fn project_onto_an_infinite_line() {
        // The line through (0,0)-(0,1) is the y-axis; projecting (5, 3)
        // lands on it at (0, 3), well outside the defining segment.
        let l = Value::Line(Line::new((0.0, 0.0), (0.0, 1.0)));
        let result = call("project", &[p(5.0, 3.0), l]).unwrap();
        approx_point(&result, 0.0, 3.0);
    }

    #[test]
    fn polar_from_origin() {
        let result = call("polar", &[p(0.0, 0.0), Value::Num(2.0), Value::Num(0.0)]).unwrap();
        approx_point(&result, 2.0, 0.0);
    }

    #[test]
    fn mirror_across_the_x_axis() {
        let l = Value::Line(Line::new((0.0, 0.0), (1.0, 0.0)));
        let result = call("mirror", &[p(3.0, 4.0), l]).unwrap();
        approx_point(&result, 3.0, -4.0);
    }

    #[test]
    fn rotate_is_counter_clockwise_in_y_up() {
        // Rotating +x by 90 degrees should land on +y (spec §5.3: "Y is
        // up. Angles are counter-clockwise from +x.").
        let t = call("rotate", &[Value::Num(std::f64::consts::FRAC_PI_2)]).unwrap();
        let result = call("apply", &[t, p(1.0, 0.0)]).unwrap();
        approx_point(&result, 0.0, 1.0);
    }

    #[test]
    fn sqrt_of_negative_is_a_domain_error() {
        assert_eq!(
            call("sqrt", &[Value::Num(-1.0)]),
            Err(EvalError::SqrtOfNegative(-1.0))
        );
    }

    #[test]
    fn asin_out_of_range_is_a_domain_error() {
        assert_eq!(
            call("asin", &[Value::Num(2.0)]),
            Err(EvalError::InverseTrigOutOfRange {
                function: "asin",
                value: 2.0
            })
        );
    }

    #[test]
    fn division_by_zero_is_a_domain_error() {
        assert_eq!(division(1.0, 0.0), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn zero_to_the_zero_is_one() {
        assert_eq!(power(0.0, 0.0), Ok(1.0));
    }

    #[test]
    fn negative_base_with_fractional_exponent_is_a_domain_error() {
        assert_eq!(
            power(-1.0, 0.5),
            Err(EvalError::PowerDomainError {
                base: -1.0,
                exponent: 0.5
            })
        );
    }

    #[test]
    fn min_of_empty_list_is_a_domain_error() {
        assert_eq!(
            call("minOf", &[Value::List(vec![])]),
            Err(EvalError::EmptyListReduction { function: "minOf" })
        );
    }

    #[test]
    fn sum_of_empty_list_is_zero() {
        assert_eq!(call("sum", &[Value::List(vec![])]), Ok(Value::Num(0.0)));
    }

    #[test]
    fn round_is_half_away_from_zero() {
        assert_eq!(call("round", &[Value::Num(2.5)]), Ok(Value::Num(3.0)));
        assert_eq!(call("round", &[Value::Num(-2.5)]), Ok(Value::Num(-3.0)));
    }

    #[test]
    fn path_query_out_of_domain_is_an_error() {
        let mut bez = kurbo::BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.line_to((1.0, 0.0));
        let path = mg_geom::skeleton::Skeleton::from_path(bez);
        let result = call("pointAt", &[Value::Path(path), Value::Num(5.0)]);
        assert_eq!(
            result,
            Err(EvalError::PathParameterOutOfDomain {
                param: 5.0,
                max: 1.0
            })
        );
    }

    fn nums(v: Value) -> Vec<f64> {
        v.as_num_list().unwrap()
    }

    fn deg(d: f64) -> Value {
        Value::Num(d.to_radians())
    }

    /// The ellipse about (0, 0) with radii 4, 2.
    fn oval() -> Value {
        call("ellipse", &[p(0.0, 0.0), Value::Num(4.0), Value::Num(2.0)]).unwrap()
    }

    #[test]
    fn ellipse_members_and_circle() {
        let c = call("circle", &[p(1.0, 2.0), Value::Num(3.0)])
            .unwrap()
            .as_ellipse()
            .unwrap();
        assert_eq!((c.center, c.rx, c.ry), (Point::new(1.0, 2.0), 3.0, 3.0));
    }

    #[test]
    fn non_positive_radius_is_a_domain_error() {
        assert_eq!(
            call("ellipse", &[p(0.0, 0.0), Value::Num(4.0), Value::Num(0.0)]),
            Err(EvalError::NonPositiveRadius(0.0))
        );
        assert_eq!(
            call("circle", &[p(0.0, 0.0), Value::Num(-1.0)]),
            Err(EvalError::NonPositiveRadius(-1.0))
        );
    }

    #[test]
    fn line_through_one_point_is_a_domain_error() {
        assert_eq!(
            call("lineThrough", &[p(1.0, 1.0), p(1.0, 1.0)]),
            Err(EvalError::LineThroughOnePoint)
        );
    }

    #[test]
    fn crossings_of_a_secant_from_outside() {
        // From (-10, 0) along +x: crosses x = -4 and x = 4.
        let l = call("lineAt", &[p(-10.0, 0.0), deg(0.0)]).unwrap();
        let hits = nums(call("crossings", &[l, oval()]).unwrap());
        assert_eq!(hits.len(), 2);
        approx(hits[0], 6.0);
        approx(hits[1], 14.0);
    }

    #[test]
    fn crossings_behind_the_origin_are_negative() {
        let l = call("lineAt", &[p(10.0, 0.0), deg(0.0)]).unwrap();
        let hits = nums(call("crossings", &[l.clone(), oval()]).unwrap());
        approx(hits[0], -14.0);
        approx(hits[1], -6.0);
        assert_eq!(
            call("cast", &[l, oval()]),
            Err(EvalError::CastMissesEllipse)
        );
    }

    #[test]
    fn cast_from_the_centre_at_each_quadrant() {
        for (angle, x, y) in [
            (0.0, 4.0, 0.0),
            (90.0, 0.0, 2.0),
            (180.0, -4.0, 0.0),
            (270.0, 0.0, -2.0),
        ] {
            let l = call("lineAt", &[p(0.0, 0.0), deg(angle)]).unwrap();
            approx_point(&call("cast", &[l, oval()]).unwrap(), x, y);
        }
    }

    #[test]
    fn cast_at_an_angle_lands_on_the_ellipse() {
        let l = call("lineAt", &[p(0.0, 0.0), deg(30.0)]).unwrap();
        let hit = call("cast", &[l, oval()]).unwrap().as_pair().unwrap();
        approx((hit.x / 4.0).powi(2) + (hit.y / 2.0).powi(2), 1.0);
        approx(hit.y.atan2(hit.x), 30f64.to_radians());
    }

    #[test]
    fn cast_from_a_point_on_the_ellipse_finds_the_far_side() {
        let l = call("lineThrough", &[p(-4.0, 0.0), p(0.0, 0.0)]).unwrap();
        approx_point(&call("cast", &[l, oval()]).unwrap(), 4.0, 0.0);
    }

    #[test]
    fn a_tangent_line_crosses_once() {
        // y = 2 touches the top of the oval at (0, 2).
        let l = call("hline", &[Value::Num(2.0)]).unwrap();
        let hits = nums(call("crossings", &[l.clone(), oval()]).unwrap());
        assert_eq!(hits.len(), 1);
        approx_point(&call("along", &[l, Value::Num(hits[0])]).unwrap(), 0.0, 2.0);
        // Within tolerance either side, too.
        for y in [2.0 + TOL / 2.0, 2.0 - TOL / 2.0] {
            let l = call("hline", &[Value::Num(y)]).unwrap();
            assert_eq!(
                nums(call("crossings", &[l, oval()]).unwrap()).len(),
                1,
                "y = {y}"
            );
        }
    }

    #[test]
    fn a_miss_has_no_crossings() {
        let l = call("hline", &[Value::Num(3.0)]).unwrap();
        assert!(nums(call("crossings", &[l.clone(), oval()]).unwrap()).is_empty());
        assert_eq!(
            call("cast", &[l, oval()]),
            Err(EvalError::CastMissesEllipse)
        );
    }

    #[test]
    fn along_measures_from_the_origin_in_units() {
        let l = call("lineThrough", &[p(1.0, 1.0), p(1.0, 11.0)]).unwrap();
        approx_point(&call("along", &[l, Value::Num(3.0)]).unwrap(), 1.0, 4.0);
    }
}
