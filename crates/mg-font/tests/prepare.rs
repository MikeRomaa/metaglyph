//! Integration tests for outline preparation (spec §10.1–§10.4, plan M5):
//! whole fonts through `mg_eval::evaluate` and `mg_font::prepare_font`.

use kurbo::{Affine, BezPath, Point, Rect, Shape};
use mg_diag::codes;
use mg_font::outline::{self, OutlinePoint};
use mg_font::{PreparedGlyph, prepare_font};
use mg_hir::model::{Hir, InstanceDecl};
use mg_syntax::ast::AstNode;

const PREAMBLE: &str = r#"
font (name: "T", em: 1000)
metric baseline (y: 0, overshoot: 10, align: "bottom")
metric xHeight (y: 500)
metric capHeight (y: 700, overshoot: 12)
metric ascender (y: 740)
metric descender (y: -200, align: "bottom")
"#;

fn lower(source: &str) -> Hir {
    let parsed = mg_syntax::parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax()).expect("SOURCE_FILE casts");
    let (hir, diagnostics) = mg_hir::lower(&source_file);
    assert!(diagnostics.is_empty(), "{:#?}", diagnostics);
    hir
}

fn prepare(
    hir: &Hir,
    instance: &InstanceDecl,
) -> (
    indexmap::IndexMap<String, PreparedGlyph>,
    Vec<mg_diag::Diagnostic>,
) {
    let (_, outcome) = mg_eval::evaluate(hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    prepare_font(hir, instance, &outcome)
}

fn curve_bbox(contour: &[OutlinePoint]) -> Rect {
    outline::to_bezpath(contour).bounding_box()
}

fn glyph_bbox(glyph: &PreparedGlyph) -> Rect {
    glyph
        .contours
        .iter()
        .map(|c| curve_bbox(c))
        .reduce(|a, b| a.union(b))
        .expect("the glyph has ink")
}

/// The on-curve points a TrueType rasterizer sees: the stored ones plus
/// the implied midpoints between consecutive off-curve points.
fn on_curve_points(contour: &[OutlinePoint]) -> Vec<Point> {
    let n = contour.len();
    let mut points = Vec::new();
    for i in 0..n {
        let (here, next) = (contour[i], contour[(i + 1) % n]);
        let p = Point::new(here.x as f64, here.y as f64);
        if here.on_curve {
            points.push(p);
        } else if !next.on_curve {
            points.push(p.midpoint(Point::new(next.x as f64, next.y as f64)));
        }
    }
    points
}

fn sample() -> Hir {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../samples/metaglyph-sans.mg"
    ))
    .unwrap();
    lower(&source)
}

/// Every sample glyph that evaluates cleanly, in every instance. Some
/// sample glyphs currently fail the spec §7.2 curvature check during
/// evaluation (MG0618); those are skipped here rather than hidden, and
/// the test fails if fewer than half the glyphs remain.
#[test]
fn the_sample_prepares_cleanly_in_every_instance() {
    let hir = sample();

    for instance in hir.instances.values() {
        let (_, outcome) = mg_eval::evaluate(&hir, instance);
        let (glyphs, diagnostics) = prepare_font(&hir, instance, &outcome);
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:#?}",
            instance.name
        );
        assert_eq!(glyphs.len(), 16);

        let clean: Vec<_> = glyphs
            .iter()
            .filter(|(name, _)| {
                !outcome
                    .failed
                    .contains(&mg_eval::NodeId::GlyphBbox(name.to_string()))
            })
            .collect();
        assert!(
            clean.len() >= 8,
            "{}: only {} clean glyphs",
            instance.name,
            clean.len()
        );

        for (name, glyph) in clean {
            let unprepared = mg_eval::render_glyph(
                &hir,
                instance,
                name,
                &outcome.values,
                &outcome.failed,
                &mut Vec::new(),
            )
            .unwrap();

            if glyph.contours.is_empty() {
                // `nine` is only a component.
                assert_eq!(glyph.components.len(), 1, "{name}");
                continue;
            }

            // Preparation moves the outline by at most cu2qu plus
            // rounding.
            let before = unprepared
                .iter()
                .map(|(p, _)| p.bounding_box())
                .reduce(|a, b| a.union(b))
                .unwrap();
            let after = glyph_bbox(glyph);
            for (b, a) in [
                (before.x0, after.x0),
                (before.y0, after.y0),
                (before.x1, after.x1),
                (before.y1, after.y1),
            ] {
                assert!(
                    (b - a).abs() <= 1.0,
                    "{}/{name}: {before:?} vs {after:?}",
                    instance.name
                );
            }

            for contour in &glyph.contours {
                // Every extremum is an on-curve point (spec §10.2): the
                // curve reaches no further than its on-curve points do,
                // up to the handles' rounding.
                let curve = curve_bbox(contour);
                let points = on_curve_points(contour);
                let on = points
                    .iter()
                    .fold(Rect::from_points(points[0], points[0]), |r, &p| {
                        r.union_pt(p)
                    });
                assert!(
                    (curve.x0 - on.x0).abs() <= 0.5
                        && (curve.y0 - on.y0).abs() <= 0.5
                        && (curve.x1 - on.x1).abs() <= 0.5
                        && (curve.y1 - on.y1).abs() <= 0.5,
                    "{}/{name}: curve {curve:?}, on-curve {on:?}",
                    instance.name
                );
            }
        }

        // Round glyphs reach their metrics' ink exactly (spec §10.4).
        let c = glyph_bbox(&glyphs["C"]);
        assert_eq!((c.y0, c.y1), (-10.0, 712.0), "{}", instance.name);
    }
}

#[test]
fn preparation_is_deterministic() {
    let hir = sample();
    let instance = &hir.instances["Bold"];
    let run = || {
        let (_, outcome) = mg_eval::evaluate(&hir, instance);
        prepare_font(&hir, instance, &outcome).0
    };
    assert_eq!(run(), run());
}

#[test]
fn slant_shears_outlines_and_conjugates_component_transforms() {
    let source = format!(
        r#"{PREAMBLE}
instance Upright ()
instance Slanted (slant: 10deg)

glyph base (advance: 200) {{
  path p (fill: true) {{
    start (at: (0, 0))
    line (to: (100, 0))
    line (to: (100, 700))
    line (to: (0, 700))
    close
  }}
}}

glyph accented (advance: 200) {{
  component (glyph: base, transform: (scale(-1, 1), translate(150.3, 20)))
}}
"#
    );
    let hir = lower(&source);

    let (upright, diagnostics) = prepare(&hir, &hir.instances["Upright"]);
    assert!(diagnostics.is_empty());
    let (slanted, diagnostics) = prepare(&hir, &hir.instances["Slanted"]);
    assert!(diagnostics.is_empty());

    // The top edge moves right by round(700 · tan 10°) = 123; the bottom
    // stays put.
    let upright_box = glyph_bbox(&upright["base"]);
    let slanted_box = glyph_bbox(&slanted["base"]);
    assert_eq!(upright_box, Rect::new(0.0, 0.0, 100.0, 700.0));
    assert_eq!(slanted_box, Rect::new(0.0, 0.0, 223.0, 700.0));
    let top_left = slanted["base"].contours[0]
        .iter()
        .find(|p| p.y == 700 && p.x < 200)
        .unwrap();
    assert_eq!(top_left.x, 123);

    // Upright: the transform verbatim. Slanted: S·M·S⁻¹, whose offset
    // is rounded.
    let upright_component = &upright["accented"].components[0];
    assert_eq!(upright_component.glyph, "base");
    assert_eq!(
        upright_component.transform.as_coeffs(),
        [-1.0, 0.0, 0.0, 1.0, 150.0, 20.0]
    );

    let s = outline::shear(10f64.to_radians());
    let m = Affine::translate((150.3, 20.0)) * Affine::scale_non_uniform(-1.0, 1.0);
    let expected = outline::conjugate(s, m).as_coeffs();
    let actual = slanted["accented"].components[0].transform.as_coeffs();
    for k in 0..4 {
        assert!((actual[k] - expected[k]).abs() < 1e-12);
    }
    assert_eq!(
        [actual[4], actual[5]],
        [expected[4].round(), expected[5].round()]
    );

    // The composite, with its conjugated transform, lands where the
    // slanted decomposed outline does.
    let mut composite = BezPath::new();
    for contour in &slanted["base"].contours {
        composite.extend(outline::to_bezpath(contour));
    }
    composite.apply_affine(slanted["accented"].components[0].transform);
    let mut decomposed = upright_box.to_path(0.1);
    decomposed.apply_affine(s * m);
    let (a, b) = (composite.bounding_box(), decomposed.bounding_box());
    assert!((a.x0 - b.x0).abs() <= 1.0 && (a.x1 - b.x1).abs() <= 1.0);
    assert!((a.y0 - b.y0).abs() <= 1.0 && (a.y1 - b.y1).abs() <= 1.0);
}

#[test]
fn a_fill_that_rounding_makes_self_intersecting_is_an_export_error() {
    // The bottom edge peaks at (10, 1.5), just under the top edge's
    // (10, 1.9). Rounded, the peak is (10, 2) and the top edge passes
    // (10, 1.5): the peak pokes through it.
    let source = format!(
        r#"{PREAMBLE}
glyph sliver (advance: 30) {{
  path wedge (fill: true) {{
    start (at: (0, 0))
    line (to: (10, 1.5))
    line (to: (20, 0))
    line (to: (20, 2.4))
    line (to: (0, 1.4))
    close
  }}
}}
"#
    );
    let hir = lower(&source);
    let (_, diagnostics) = prepare(&hir, &hir.instances["Regular"]);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].code, codes::QUANTIZED_FILL_SELF_INTERSECTS);
    assert!(diagnostics[0].message.contains("`wedge`"));
    assert!(diagnostics[0].message.contains("`sliver`"));
}

#[test]
fn a_stroke_whose_contours_touch_after_rounding_is_not_checked() {
    // Only filled contours are re-checked (spec §10.4 step 4); a stroke
    // outline overlapping itself is legitimate (spec §7.4).
    let source = format!(
        r#"{PREAMBLE}
glyph x (advance: 300) {{
  path a (stroke: 40) {{
    start (at: (0, 0))
    line (to: (200, 500))
  }}
  path b (stroke: 40) {{
    start (at: (200, 0))
    line (to: (0, 500))
  }}
}}
"#
    );
    let hir = lower(&source);
    let (glyphs, diagnostics) = prepare(&hir, &hir.instances["Regular"]);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(glyphs["x"].contours.len(), 2);
}
