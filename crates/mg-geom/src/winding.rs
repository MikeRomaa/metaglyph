//! Contour roles and winding direction (spec §8). Signed area and
//! point-in-contour containment are exactly kurbo's own `Shape::area` /
//! `Shape::winding` on a closed [`BezPath`] — nothing to reimplement —
//! so this module only adds the decisions spec §8 builds on top of them:
//! which of two contours encloses the other, a glyph's own filled
//! contours' nesting count, and how a role's winding direction should
//! read for `glyf`.

use kurbo::{BezPath, PathEl, Point, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContourRole {
    Outer,
    Counter,
}

/// `true` when `path`'s own signed area reads clockwise (negative, in
/// this y-up coordinate system — spec §14 "y-up" throughout).
pub fn is_clockwise(path: &BezPath) -> bool {
    path.area() < 0.0
}

/// `glyf` (TrueType, y-up) winding per spec §8.2: outer clockwise,
/// counter counter-clockwise. CFF/CFF2 uses the opposite convention, but
/// is out of scope for this project (`plans/3-rust-impl.md`: "Output
/// formats: TTF only").
pub fn glyf_direction_is_clockwise(role: ContourRole) -> bool {
    match role {
        ContourRole::Outer => true,
        ContourRole::Counter => false,
    }
}

/// Reverses `path`'s winding direction in place if it disagrees with
/// `role`'s `glyf` convention (spec §8.2).
pub fn orient_for_glyf(path: &mut BezPath, role: ContourRole) {
    if is_clockwise(path) != glyf_direction_is_clockwise(role) {
        *path = path.reverse_subpaths();
    }
}

fn first_point(path: &BezPath) -> Point {
    match path.elements().first() {
        Some(PathEl::MoveTo(p)) => *p,
        _ => panic!("a contour must start with `MoveTo`"),
    }
}

/// Does `outer` enclose `inner`? Tested by evaluating `outer`'s winding
/// number at one of `inner`'s own points (spec §8.1: "the one enclosing
/// the other is outer... Decide by point-in-contour, not by kurbo's
/// output order").
pub fn encloses(outer: &BezPath, inner: &BezPath) -> bool {
    outer.winding(first_point(inner)) != 0
}

/// A stroked closed path's two kurbo-generated subpaths, given their
/// roles by containment (spec §8.1) rather than kurbo's emission order.
/// Panics if neither encloses the other, which does not arise for a
/// simple (non-self-intersecting) closed skeleton — kurbo always emits
/// exactly one enclosing subpath and one enclosed.
pub fn stroke_closed_roles(a: &BezPath, b: &BezPath) -> [(ContourRole, ContourRole); 1] {
    if encloses(a, b) {
        [(ContourRole::Outer, ContourRole::Counter)]
    } else if encloses(b, a) {
        [(ContourRole::Counter, ContourRole::Outer)]
    } else {
        unreachable!("a stroked closed path's two subpaths always nest one inside the other")
    }
}

/// Among `contours` (a glyph's own filled contours, spec §8.1), each
/// one's final role: `Counter` when enclosed by an odd number of the
/// others, `Outer` otherwise. Stroked contours and component contours
/// take no part in this count (spec §8.1) — the caller filters to just
/// the filled ones before calling this.
pub fn resolve_fill_roles(contours: &[BezPath]) -> Vec<ContourRole> {
    contours
        .iter()
        .enumerate()
        .map(|(i, contour)| {
            let enclosing_count = contours
                .iter()
                .enumerate()
                .filter(|&(j, other)| j != i && encloses(other, contour))
                .count();
            if enclosing_count % 2 == 1 {
                ContourRole::Counter
            } else {
                ContourRole::Outer
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ccw_square(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        // Rect::to_path direction depends on kurbo's own convention; build
        // an explicit CCW square instead so the test's expectations don't
        // depend on that.
        let mut p = BezPath::new();
        p.move_to((x0, y0));
        p.line_to((x1, y0));
        p.line_to((x1, y1));
        p.line_to((x0, y1));
        p.close_path();
        p
    }

    fn cw_square(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((x0, y0));
        p.line_to((x0, y1));
        p.line_to((x1, y1));
        p.line_to((x1, y0));
        p.close_path();
        p
    }

    #[test]
    fn signed_area_sign_matches_winding_direction() {
        assert!(!is_clockwise(&ccw_square(0.0, 0.0, 10.0, 10.0)));
        assert!(is_clockwise(&cw_square(0.0, 0.0, 10.0, 10.0)));
    }

    #[test]
    fn orient_for_glyf_flips_only_when_needed() {
        let mut outer = ccw_square(0.0, 0.0, 10.0, 10.0);
        orient_for_glyf(&mut outer, ContourRole::Outer);
        assert!(is_clockwise(&outer)); // outer must end up clockwise

        let mut counter = cw_square(0.0, 0.0, 10.0, 10.0);
        orient_for_glyf(&mut counter, ContourRole::Counter);
        assert!(!is_clockwise(&counter)); // counter must end up ccw
    }

    #[test]
    fn encloses_uses_containment_not_emission_order() {
        let outer = ccw_square(0.0, 0.0, 10.0, 10.0);
        let inner = ccw_square(3.0, 3.0, 7.0, 7.0);
        assert!(encloses(&outer, &inner));
        assert!(!encloses(&inner, &outer));
    }

    #[test]
    fn stroke_closed_roles_finds_the_enclosing_one_either_order() {
        let outer = ccw_square(0.0, 0.0, 10.0, 10.0);
        let inner = ccw_square(3.0, 3.0, 7.0, 7.0);
        let [(role_a, role_b)] = stroke_closed_roles(&outer, &inner);
        assert_eq!(role_a, ContourRole::Outer);
        assert_eq!(role_b, ContourRole::Counter);

        let [(role_a, role_b)] = stroke_closed_roles(&inner, &outer);
        assert_eq!(role_a, ContourRole::Counter);
        assert_eq!(role_b, ContourRole::Outer);
    }

    #[test]
    fn fill_nesting_counts_odd_enclosure_as_counter() {
        let a = ccw_square(0.0, 0.0, 30.0, 30.0);
        let b = ccw_square(5.0, 5.0, 25.0, 25.0);
        let c = ccw_square(10.0, 10.0, 20.0, 20.0);
        let roles = resolve_fill_roles(&[a, b, c]);
        assert_eq!(
            roles,
            vec![ContourRole::Outer, ContourRole::Counter, ContourRole::Outer,]
        );
    }
}
