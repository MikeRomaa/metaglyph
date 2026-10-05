//! Integration tests for `mg_eval::evaluate` (spec §4): build real HIR
//! from source text, evaluate it for the (implicit) `Regular` instance,
//! and check the results.

use mg_eval::NodeId;
use mg_eval::value::Value;
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

fn num(values: &indexmap::IndexMap<NodeId, Value>, node: NodeId) -> f64 {
    values
        .get(&node)
        .unwrap_or_else(|| panic!("no value for {node}"))
        .as_num()
        .unwrap_or_else(|| panic!("{node} is not a number: {:?}", values.get(&node)))
}

#[test]
fn params_metrics_and_lets_evaluate() {
    let source =
        format!("{PREAMBLE}\nparam stem (default: 100, range: 20..260)\nlet hair = stem * 0.5;\n");
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    assert_eq!(num(&outcome.values, NodeId::TopLevel("stem".into())), 100.0);
    assert_eq!(num(&outcome.values, NodeId::TopLevel("hair".into())), 50.0);
    let baseline = outcome.values[&NodeId::TopLevel("baseline".into())]
        .as_zone()
        .unwrap();
    assert_eq!(baseline.y, 0.0);
    assert_eq!(baseline.ink, 0.0);
    let x_height = outcome.values[&NodeId::TopLevel("xHeight".into())]
        .as_zone()
        .unwrap();
    assert_eq!(x_height.ink, 500.0); // default align "top": ink = y + overshoot = 500 + 0
}

#[test]
fn instance_override_replaces_the_default() {
    let source = format!(
        "{PREAMBLE}\nparam stem (default: 100, range: 20..260)\ninstance Bold (stem: 200)\n"
    );
    let hir = lower(&source);
    let instance = hir.instances.get("Bold").unwrap();
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(num(&outcome.values, NodeId::TopLevel("stem".into())), 200.0);
}

#[test]
fn glyph_advance_path_and_anchor_evaluate() {
    let source = format!(
        r#"{PREAMBLE}
let w = 5;
glyph A (advance: w * 2 + 10) {{
  path guide () {{
    start (at: (0, 0))
    line (to: (w, 0))
  }}
  anchor top (at: (w, xHeight.y))
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    // `path guide` has neither `stroke` nor `fill`, so `A` has no ink.
    // That is legal; only reading `A.bbox` would be an error.
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.values[&NodeId::GlyphBbox("A".into())],
        mg_eval::value::Value::NoInk
    );

    assert_eq!(num(&outcome.values, NodeId::GlyphAdvance("A".into())), 20.0);
    let anchor = outcome.values[&NodeId::Anchor("A".into(), "top".into())]
        .as_pair()
        .unwrap();
    assert_eq!((anchor.x, anchor.y), (5.0, 500.0));

    let bbox = outcome.values[&NodeId::PathBbox("A".into(), 0)]
        .as_rect()
        .unwrap();
    assert_eq!((bbox.x0, bbox.y0, bbox.x1, bbox.y1), (0.0, 0.0, 5.0, 0.0));
}

#[test]
fn glyph_bbox_of_an_empty_glyph_is_a_domain_error() {
    let source =
        format!("{PREAMBLE}\nglyph A (advance: 10) {{ anchor a (at: (glyph.bbox.x1, 0)) }}\n");
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);

    // The glyph itself is fine; the read of its `.bbox` is what fails.
    assert!(!outcome.failed.contains(&NodeId::GlyphBbox("A".into())));
    assert!(
        outcome
            .failed
            .contains(&NodeId::Anchor("A".into(), "a".into()))
    );
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.diagnostics[0].code,
        mg_diag::codes::GLYPH_HAS_NO_INK
    );
}

#[test]
fn rendering_path_bbox_is_the_stroked_outline() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path p (stroke: 5) {{
    start (at: (0, 0))
    line (to: (10, 0))
  }}
}}
let w = glyphs.A.bbox.x1;
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    // A horizontal line stroked with the default "butt" caps: the offset
    // boundary is exactly the line's own extent in x, grown by `r = 2.5`
    // in y, with no extension past the endpoints.
    let bbox = outcome.values[&NodeId::PathBbox("A".into(), 0)]
        .as_rect()
        .unwrap();
    assert_eq!((bbox.x0, bbox.y0, bbox.x1, bbox.y1), (0.0, -2.5, 10.0, 2.5));
    assert_eq!(num(&outcome.values, NodeId::TopLevel("w".into())), 10.0);
}

#[test]
fn tight_curvature_is_trimmed_silently() {
    // A radius-2 circle stroked at 20 (spec §7.2): the pen fills the
    // counter, so the ink is a solid disc of radius 12, with no
    // diagnostic.
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path p (stroke: 20) {{
    start (at: (2, 0))
    arc (to: (-2, 0), center: (0, 0), sweep: "ccw")
    arc (to: (2, 0),  center: (0, 0), sweep: "ccw")
  }}
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(!outcome.failed.contains(&NodeId::PathBbox("A".into(), 0)));
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let bbox = outcome.values[&NodeId::PathBbox("A".into(), 0)]
        .as_rect()
        .unwrap();
    assert!((bbox.x1 - bbox.x0 - 24.0).abs() < 0.1, "{bbox:?}");

    let contours = mg_eval::render_glyph(
        &hir,
        instance,
        "A",
        &outcome.values,
        &outcome.failed,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(contours.len(), 1, "the counter is filled");
}

#[test]
fn an_interior_cusp_is_an_error() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path p (stroke: 2) {{
    start (at: (0, 0))
    cube (c1: (1, 0), c2: (0.5, -0.5), to: (0.5, 0.5))
  }}
}}
"#
    );
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(outcome.failed.contains(&NodeId::PathBbox("A".into(), 0)));
    assert_eq!(outcome.diagnostics[0].code, mg_diag::codes::INTERIOR_CUSP);
}

#[test]
fn a_self_intersecting_fill_is_an_error() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path p (fill: true) {{
    start (at: (-10, 10))
    line (to: (10, -10))
    line (to: (10, 10))
    line (to: (-10, -10))
    close
  }}
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.failed.contains(&NodeId::PathBbox("A".into(), 0)));
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.diagnostics[0].code,
        mg_diag::codes::SELF_INTERSECTING_FILL
    );
}

#[test]
fn failure_containment_leaves_independent_nodes_evaluated() {
    let source =
        format!("{PREAMBLE}\nlet bad = 1 / 0;\nlet dependent = bad + 1;\nlet independent = 42;\n");
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);

    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.diagnostics[0].code,
        mg_diag::codes::DIVISION_BY_ZERO
    );
    assert!(outcome.failed.contains(&NodeId::TopLevel("bad".into())));
    assert!(
        outcome
            .failed
            .contains(&NodeId::TopLevel("dependent".into()))
    );
    assert_eq!(
        num(&outcome.values, NodeId::TopLevel("independent".into())),
        42.0
    );
}

#[test]
fn a_self_cycle_is_reported_as_itself() {
    let source = format!("{PREAMBLE}\nlet a = a + 1;\n");
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].code, mg_diag::codes::CYCLE);
    assert!(outcome.failed.contains(&NodeId::TopLevel("a".into())));
}

#[test]
fn a_long_cycle_is_reported_with_every_hop() {
    let source = format!("{PREAMBLE}\nlet a = b + 1;\nlet b = c + 1;\nlet c = a + 1;\n");
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].code, mg_diag::codes::CYCLE);
    for name in ["a", "b", "c"] {
        assert!(
            outcome.failed.contains(&NodeId::TopLevel(name.into())),
            "{name} should be failed"
        );
    }
}

/// The M3 acceptance criterion, stated directly in the spec plan:
/// "permuting statements within a scope provably changes nothing."
/// Exhaustive over every ordering of four interdependent `let`s (24
/// permutations) — stronger than a random sample, and there is no
/// randomness to seed.
#[test]
fn permuting_top_level_lets_never_changes_the_result() {
    let lines = [
        "let a = stem + 1;",
        "let b = a + 1;",
        "let c = b + 1;",
        "let d = a + c;",
    ];
    let mut expected: Option<[f64; 4]> = None;

    for perm in permutations(&lines) {
        let mut source = format!("{PREAMBLE}\nparam stem (default: 10, range: 0..100)\n");
        for line in &perm {
            source.push_str(line);
            source.push('\n');
        }
        let hir = lower(&source);
        let instance = regular(&hir);
        let (_, outcome) = mg_eval::evaluate(&hir, instance);
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

        let got = [
            num(&outcome.values, NodeId::TopLevel("a".into())),
            num(&outcome.values, NodeId::TopLevel("b".into())),
            num(&outcome.values, NodeId::TopLevel("c".into())),
            num(&outcome.values, NodeId::TopLevel("d".into())),
        ];
        match &expected {
            None => expected = Some(got),
            Some(want) => assert_eq!(&got, want, "order {perm:?} produced a different result"),
        }
    }
}

fn permutations<'a>(items: &[&'a str; 4]) -> Vec<[&'a str; 4]> {
    let mut indices = [0, 1, 2, 3];
    let mut out = Vec::new();
    permute(&mut indices, 0, &mut |p| {
        out.push([items[p[0]], items[p[1]], items[p[2]], items[p[3]]]);
    });
    out
}

fn permute(arr: &mut [usize; 4], k: usize, visit: &mut impl FnMut(&[usize; 4])) {
    if k == arr.len() {
        visit(arr);
        return;
    }
    for i in k..arr.len() {
        arr.swap(k, i);
        permute(arr, k + 1, visit);
        arr.swap(k, i);
    }
}

#[test]
fn dirty_set_reevaluation_only_touches_the_downstream_subgraph() {
    let source = format!(
        "{PREAMBLE}\nparam stem (default: 10, range: 0..100)\nlet a = stem + 1;\nlet b = a + 1;\nlet unrelated = 999;\n"
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (graph, first) = mg_eval::evaluate(&hir, instance);

    assert_eq!(num(&first.values, NodeId::TopLevel("a".into())), 11.0);
    assert_eq!(num(&first.values, NodeId::TopLevel("b".into())), 12.0);
    assert_eq!(
        num(&first.values, NodeId::TopLevel("unrelated".into())),
        999.0
    );

    // Poison `unrelated`'s cached value; if `reevaluate` recomputes it
    // instead of reusing the cache, this proves it by disagreeing with
    // the poisoned value below.
    let mut poisoned = first;
    poisoned
        .values
        .insert(NodeId::TopLevel("unrelated".into()), Value::Num(-1.0));

    let closure = mg_eval::dirty_closure(&graph, &NodeId::TopLevel("stem".into()));
    assert!(closure.contains(&NodeId::TopLevel("stem".into())));
    assert!(closure.contains(&NodeId::TopLevel("a".into())));
    assert!(closure.contains(&NodeId::TopLevel("b".into())));
    assert!(!closure.contains(&NodeId::TopLevel("unrelated".into())));

    let updated = mg_eval::reevaluate(
        &hir,
        instance,
        &graph,
        &poisoned,
        &NodeId::TopLevel("stem".into()),
    );
    assert_eq!(num(&updated.values, NodeId::TopLevel("stem".into())), 10.0);
    assert_eq!(num(&updated.values, NodeId::TopLevel("a".into())), 11.0);
    assert_eq!(num(&updated.values, NodeId::TopLevel("b".into())), 12.0);
    // Untouched — still the poisoned value, proving it was never
    // recomputed.
    assert_eq!(
        num(&updated.values, NodeId::TopLevel("unrelated".into())),
        -1.0
    );
}

#[test]
fn intersect_of_two_line_only_paths() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (advance: 10) {{
  path h () {{ start (at: (0, 5)) line (to: (10, 5)) }}
  path v () {{ start (at: (5, 0)) line (to: (5, 10)) }}
  let hits = intersect(h, v);
  let total = sum(hits);
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    // Neither path renders, which is legal (see
    // `glyph_advance_path_and_anchor_evaluate`).
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert!(!outcome.failed.contains(&NodeId::GlyphBbox("A".into())));
    let hits = outcome.values[&NodeId::GlyphLocal("A".into(), "hits".into())]
        .as_num_list()
        .unwrap();
    // The vertical line crosses the horizontal one exactly once, at
    // parameter 0.5 along it (the horizontal line spans x in [0, 10]
    // and the crossing is at x = 5).
    assert_eq!(hits, vec![0.5]);
}

#[test]
fn a_cancelled_evaluation_stops_between_nodes() {
    let source = format!("{PREAMBLE}\nlet a = 1;\nlet b = a + 1;\nlet c = b + 1;\n");
    let hir = lower(&source);
    let instance = regular(&hir);

    assert!(mg_eval::evaluate_cancellable(&hir, instance, &|| true).is_none());

    // Cancelled partway: after two nodes have been checked.
    let checks = std::cell::Cell::new(0);
    let partway = mg_eval::evaluate_cancellable(&hir, instance, &|| {
        checks.set(checks.get() + 1);
        checks.get() > 2
    });
    assert!(partway.is_none());
    assert_eq!(checks.get(), 3);

    // Never cancelled: the same values as `evaluate`.
    let (_, full) = mg_eval::evaluate(&hir, instance);
    let (_, uncancelled) = mg_eval::evaluate_cancellable(&hir, instance, &|| false).unwrap();
    assert_eq!(full.values, uncancelled.values);
}

/// A filled 100×100 square with its left edge at x = 10.
const SQUARE: &str = r#"
  path box (fill: true) {
    start (at: (10, 0))
    line (to: (110, 0))
    line (to: (110, 100))
    line (to: (10, 100))
    close
  }
"#;

/// `(advance, shift)` for glyph `A` declared with `spacing`.
fn spacing_of(spacing: &str) -> (f64, f64) {
    let source = format!("{PREAMBLE}\nglyph A ({spacing}) {{{SQUARE}}}\n");
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    (
        num(&outcome.values, NodeId::GlyphAdvance("A".into())),
        num(&outcome.values, NodeId::GlyphShift("A".into())),
    )
}

#[test]
fn spacing_fields_set_advance_and_shift() {
    // Spec §12.1's table, on ink spanning x = 10..110.
    assert_eq!(spacing_of("advance: 300"), (300.0, 0.0));
    assert_eq!(spacing_of("rsb: 30"), (140.0, 0.0));
    assert_eq!(spacing_of("lsb: 20"), (140.0, 10.0));
    assert_eq!(spacing_of("lsb: 20, rsb: 50"), (170.0, 10.0));
    assert_eq!(spacing_of("advance: 300, lsb: 20"), (300.0, 10.0));
    assert_eq!(spacing_of("advance: 200, rsb: 50"), (200.0, 40.0));
}

#[test]
fn monospace_centring_reads_its_own_advance() {
    assert_eq!(
        spacing_of("advance: 300, lsb: (glyph.advance - glyph.bbox.width) / 2"),
        (300.0, 90.0)
    );
}

#[test]
fn a_bearing_derived_from_the_advance_it_derives_is_a_cycle() {
    let source = format!("{PREAMBLE}\nglyph A (lsb: glyph.advance / 4) {{{SQUARE}}}\n");
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "MG0601"),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn a_bearing_on_an_inkless_glyph_is_a_domain_error() {
    let source = format!("{PREAMBLE}\nglyph space (rsb: 200) {{}}\n");
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(
        outcome
            .failed
            .contains(&NodeId::GlyphAdvance("space".into()))
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code == mg_diag::codes::GLYPH_HAS_NO_INK),
        "{:#?}",
        outcome.diagnostics
    );
}

#[test]
fn other_glyphs_are_read_placed_and_the_own_glyph_authored() {
    let source = format!(
        r#"{PREAMBLE}
glyph A (lsb: 20) {{{SQUARE}
  anchor top (at: (60, 100))
  anchor own (at: (glyph.bbox.x0, top.x))
}}
glyph B (advance: 10) {{
  anchor a (at: (glyphs.A.bbox.x0, glyphs.A.top.x))
}}
glyph C (rsb: 0) {{
  component (glyph: A)
}}
"#
    );
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let pair = |glyph: &str, anchor: &str| {
        let p = outcome.values[&NodeId::Anchor(glyph.into(), anchor.into())]
            .as_pair()
            .unwrap();
        (p.x, p.y)
    };
    // `A` is shifted by 10: inside it nothing moves; from `B` everything has.
    assert_eq!(pair("A", "own"), (10.0, 60.0));
    assert_eq!(pair("B", "a"), (20.0, 70.0));

    // `C` draws `A` placed, so its own ink starts at 20.
    let c = outcome.values[&NodeId::GlyphBbox("C".into())]
        .as_rect()
        .unwrap();
    assert_eq!((c.x0, c.x1), (20.0, 120.0));
    assert_eq!(
        num(&outcome.values, NodeId::GlyphAdvance("C".into())),
        120.0
    );
}

#[test]
fn outlines_are_placed() {
    use kurbo::Shape;

    let source = format!(
        r#"{PREAMBLE}
glyph A (lsb: 20) {{{SQUARE}}}
glyph C (lsb: 0) {{
  component (glyph: A, offset: (5, 0))
}}
"#
    );
    let hir = lower(&source);
    let instance = regular(&hir);
    let (_, outcome) = mg_eval::evaluate(&hir, instance);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);

    let outline = |name: &str| {
        mg_eval::glyph_outline(
            &hir,
            instance,
            name,
            &outcome.values,
            &outcome.failed,
            &mut Vec::new(),
        )
    };
    let a = outline("A");
    let bounds = a.contours[0].path.bounding_box();
    assert_eq!((bounds.x0, bounds.x1), (20.0, 120.0));

    // `C`'s authored ink is `A` placed (20..120) plus the offset: 25..125.
    // `lsb: 0` shifts it by -25 on top of the offset.
    let c = outline("C");
    assert_eq!(c.components[0].transform.translation().x, 5.0 - 25.0);
}

#[test]
fn a_ray_cast_onto_an_ellipse() {
    let source = format!(
        r#"{PREAMBLE}
glyph O (advance: 600) {{
  let e = ellipse((300, 350), 250, 300);
  let top = cast(lineAt(e.center, 90deg), e);
  let l = hline(350);
  let east = along(l, maxOf(crossings(l, e)));
  let rx = e.rx;
}}
"#
    );
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    let local = |name: &str| NodeId::GlyphLocal("O".into(), name.into());
    let pair = |name: &str| outcome.values[&local(name)].as_pair().unwrap();
    let top = pair("top");
    assert!((top.x - 300.0).abs() < 1e-9 && (top.y - 650.0).abs() < 1e-9, "{top:?}");
    let east = pair("east");
    assert!((east.x - 550.0).abs() < 1e-9 && (east.y - 350.0).abs() < 1e-9, "{east:?}");
    assert_eq!(num(&outcome.values, local("rx")), 250.0);
}

#[test]
fn a_narrow_filled_and_stroked_path_evaluates_cleanly() {
    // The `!` in samples/a22x-mono.mg with `fill: true`: its arcs' piece
    // joints are not self-crossings (spec §8.3), and the stroke's tight
    // vertices are trimmed silently (spec §7.2).
    let source = format!(
        r#"{PREAMBLE}
glyph exclam (advance: 500) {{
  let upper = ellipse((250, 880), 60, 120);
  let lower = ellipse((250, 500), 25, 50);
  let upper0 = cast(lineAt(upper.center, 185deg), upper);
  let upper1 = cast(lineAt(upper.center, -5deg), upper);
  let lower0 = cast(lineAt(lower.center, -5deg), lower);
  let lower1 = cast(lineAt(lower.center, 185deg), lower);
  path top (stroke: 100, joins: "round", fill: true) {{
    start (at: upper0)
    arc   (to: upper1, rx: 60, ry: 120, sweep: "cw", large: true)
    line  (to: lower0)
    arc   (to: lower1, rx: 25, ry: 50, sweep: "cw")
    close
  }}
}}
"#
    );
    let hir = lower(&source);
    let (_, outcome) = mg_eval::evaluate(&hir, regular(&hir));
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}
