//! The construction library (spec §5.9): every callable name, as a real
//! computation over already-evaluated [`Value`]s. `mg-hir` (M2) already
//! statically checked each call's arity and argument types against this
//! same signature table (`mg_hir::types::lookup_function`), so a shape
//! mismatch here — wrong count, wrong `Value` variant — is a bug
//! upstream, not a user-facing error; only genuine spec §13 domain
//! errors are [`EvalError`]s.

use kurbo::{Affine, Line, ParamCurve, ParamCurveArclen, ParamCurveExtrema, Point, Vec2};
use mg_geom::skeleton;

use crate::errors::EvalError;
use crate::value::{Rect, Value};

/// Arc-length numerics have no spec-named tolerance (unlike the M4
/// constants in spec §14); this is just "close enough" for `mg dump-graph`
/// and M3's own tests.
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

fn transform(v: &Value) -> Affine {
    v.as_transform()
        .expect("mg-hir already type-checked this argument as `transform`")
}

fn path(v: &Value) -> &kurbo::BezPath {
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
/// upstream, not a domain error.
pub fn call(name: &str, args: &[Value]) -> Result<Value, EvalError> {
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

        "lineThrough" => Ok(Value::Line(Line::new(pair(&args[0]), pair(&args[1])))),
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
            path(&args[0]),
            ARC_ACCURACY,
        ))),
        "pointAtLength" => Ok(Value::Pair(point_at_length(path(&args[0]), num(&args[1])))),
        "intersect" => {
            let hits = skeleton::intersect(path(&args[0]), path(&args[1]))?;
            Ok(Value::List(hits.into_iter().map(Value::Num).collect()))
        }
        "subpath" => {
            let (t0, t1) = (num(&args[1]), num(&args[2]));
            Ok(Value::Path(subpath(path(&args[0]), t0, t1)?))
        }
        "reverse" => Ok(Value::Path(path(&args[0]).reverse_subpaths())),
        "extrema" => Ok(Value::List(
            path_extrema(path(&args[0]))
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

/// `t`'s segment and local parameter (spec §5.9: domain `[0, n]`,
/// segment `i` spans `[i, i+1]`).
fn resolve_param(bez: &kurbo::BezPath, t: f64) -> Result<(kurbo::PathSeg, f64), EvalError> {
    let segs = skeleton::segments(bez);
    let n = segs.len();
    if n == 0 || t < 0.0 || t > n as f64 {
        return Err(EvalError::PathParameterOutOfDomain {
            param: t,
            max: n as f64,
        });
    }
    let i = if t >= n as f64 {
        n - 1
    } else {
        t.floor() as usize
    };
    Ok((segs[i], t - i as f64))
}

fn point_at_length(bez: &kurbo::BezPath, s: f64) -> Point {
    let mut remaining = s;
    let segs = skeleton::segments(bez);
    for seg in &segs {
        let len = seg.arclen(ARC_ACCURACY);
        if remaining <= len || std::ptr::eq(seg, segs.last().unwrap()) {
            let t = seg.inv_arclen(remaining.max(0.0), ARC_ACCURACY);
            return seg.eval(t);
        }
        remaining -= len;
    }
    bez.segments()
        .last()
        .map(|s| s.eval(1.0))
        .unwrap_or_default()
}

fn subpath(bez: &kurbo::BezPath, t0: f64, t1: f64) -> Result<kurbo::BezPath, EvalError> {
    let segs = skeleton::segments(bez);
    let n = segs.len();
    if n == 0 || t0 < 0.0 || t1 > n as f64 {
        return Err(EvalError::PathParameterOutOfDomain {
            param: t0.max(t1),
            max: n as f64,
        });
    }
    let mut out = kurbo::BezPath::new();
    let (start_seg, start_t) = resolve_param(bez, t0)?;
    let start_point = start_seg.eval(start_t);
    out.move_to(start_point);

    let i0 = t0.floor() as usize;
    let i1 = if t1 >= n as f64 {
        n - 1
    } else {
        t1.floor() as usize
    };
    for (i, seg) in segs.iter().enumerate().take(i1 + 1).skip(i0) {
        let lo = if i == i0 { start_t } else { 0.0 };
        let hi = if i == i1 { t1 - i as f64 } else { 1.0 };
        match seg.subsegment(lo..hi) {
            kurbo::PathSeg::Line(l) => out.line_to(l.p1),
            kurbo::PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            kurbo::PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
    }
    Ok(out)
}

fn path_extrema(bez: &kurbo::BezPath) -> Vec<f64> {
    skeleton::segments(bez)
        .iter()
        .enumerate()
        .flat_map(|(i, seg)| match seg {
            kurbo::PathSeg::Line(l) => l
                .extrema()
                .into_iter()
                .map(move |t| i as f64 + t)
                .collect::<Vec<_>>(),
            kurbo::PathSeg::Quad(q) => q
                .extrema()
                .into_iter()
                .map(move |t| i as f64 + t)
                .collect::<Vec<_>>(),
            kurbo::PathSeg::Cubic(c) => c
                .extrema()
                .into_iter()
                .map(move |t| i as f64 + t)
                .collect::<Vec<_>>(),
        })
        .collect()
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
        let result = call("pointAt", &[Value::Path(bez), Value::Num(5.0)]);
        assert_eq!(
            result,
            Err(EvalError::PathParameterOutOfDomain {
                param: 5.0,
                max: 1.0
            })
        );
    }
}
