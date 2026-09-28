//! Integration tests for `mg_eval::render_glyph` (spec §6.5, §8.1): the
//! glyph-level orchestration on top of `mg_eval::eval::render_path` — fill
//! nesting across a glyph's own paths, and component placement.

use mg_geom::winding::ContourRole;
use mg_hir::model::Hir;
use mg_syntax::ast::AstNode;

const PREAMBLE: &str = r#"
font (name: "T", em: 1000)
metric baseline (y: 0, align: "bottom")
metric xHeight (y: 500)
metric capHeight (y: 700)
metric ascender (y: 740)
metric descender (y: -200)
"#;

fn lower(source: &str) -> Hir {
    let parsed = mg_syntax::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "syntax errors: {:?}",
        parsed.diagnostics
    );
    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax()).expect("SOURCE_FILE casts");
    let (hir, diagnostics) = mg_hir::lower(&source_file);
    assert!(diagnostics.is_empty(), "HIR errors: {:#?}", diagnostics);
    hir
}

fn regular(hir: &Hir) -> &mg_hir::model::InstanceDecl {
    hir.instances
        .get("Regular")
        .expect("the implicit Regular instance")
}

#[test]
fn a_stroked_open_path_renders_one_outer_contour() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path p (stroke: 2) {{
    start (at: (0, 0))
    line (to: (10, 0))
  }}
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut diagnostics = Vec::new();
    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        "A",
        &outcome.values,
        &outcome.failed,
        &mut diagnostics,
    )
    .unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(contours.len(), 1);
    assert_eq!(contours[0].1, ContourRole::Outer);
}

#[test]
fn nested_filled_squares_resolve_counter_by_odd_enclosure() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 30) {{
  path outer (fill: true) {{
    start (at: (0, 0))
    line (to: (30, 0))
    line (to: (30, 30))
    line (to: (0, 30))
    close
  }}
  path middle (fill: true) {{
    start (at: (5, 5))
    line (to: (25, 5))
    line (to: (25, 25))
    line (to: (5, 25))
    close
  }}
  path inner (fill: true) {{
    start (at: (10, 10))
    line (to: (20, 10))
    line (to: (20, 20))
    line (to: (10, 20))
    close
  }}
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut diagnostics = Vec::new();
    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        "A",
        &outcome.values,
        &outcome.failed,
        &mut diagnostics,
    )
    .unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(contours.len(), 3);
    let roles: Vec<ContourRole> = contours.iter().map(|(_, role)| *role).collect();
    assert_eq!(
        roles,
        vec![ContourRole::Outer, ContourRole::Counter, ContourRole::Outer,]
    );
}

#[test]
fn a_component_places_the_target_glyphs_contours() {
    let source = format!(
        r#"{PREAMBLE}
glyph base (advance: 10) {{
  path p (stroke: 2) {{
    start (at: (0, 0))
    line (to: (10, 0))
  }}
}}
glyph composed (advance: 10) {{
  component (glyph: base, offset: (5, 5))
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let mut diagnostics = Vec::new();
    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        "composed",
        &outcome.values,
        &outcome.failed,
        &mut diagnostics,
    )
    .unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(contours.len(), 1);

    use kurbo::Shape;
    let base_contours = mg_eval::render_glyph(
        &hir,
        instance,
        "base",
        &outcome.values,
        &outcome.failed,
        &mut Vec::new(),
    )
    .unwrap();
    let base_bbox = base_contours[0].0.bounding_box();
    let composed_bbox = contours[0].0.bounding_box();
    // The component's `offset: (5, 5)` shifts the base glyph's contour by
    // (5, 5); winding direction may differ from the un-oriented base
    // render, so compare extents rather than exact points.
    assert_eq!(composed_bbox.width(), base_bbox.width());
    assert_eq!(composed_bbox.height(), base_bbox.height());
    assert_eq!(composed_bbox.x0, base_bbox.x0 + 5.0);
    assert_eq!(composed_bbox.y0, base_bbox.y0 + 5.0);
}

#[test]
fn a_broken_path_is_skipped_not_a_panic() {
    // `bar0`/`bar1` are the same point: `bar`'s `PathRealized` fails with
    // `ZeroLengthSegment`, so it never lands in `outcome.values`. This
    // must not panic — `good` renders fine and `bar` is silently skipped,
    // exactly like `.bbox`'s own failure containment (spec §4.6), but
    // without aborting the whole glyph the way `GlyphBbox`'s all-or-
    // nothing dependency on every one of its own paths would.
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path good (stroke: 2) {{
    start (at: (0, 0))
    line (to: (10, 0))
  }}
  path bar (stroke: 2) {{
    start (at: (5, 5))
    line (to: (5, 5))
  }}
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.diagnostics[0].code,
        mg_diag::codes::ZERO_LENGTH_SEGMENT
    );

    let mut diagnostics = Vec::new();
    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        "A",
        &outcome.values,
        &outcome.failed,
        &mut diagnostics,
    )
    .unwrap();
    // No new diagnostic here: the one above already reported it.
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(contours.len(), 1);
    assert_eq!(contours[0].1, ContourRole::Outer);
}
