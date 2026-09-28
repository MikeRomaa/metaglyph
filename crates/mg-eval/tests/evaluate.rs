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
fn a_curvature_violation_is_reported_before_stroking() {
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
    assert!(outcome.failed.contains(&NodeId::PathBbox("A".into(), 0)));
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(
        outcome.diagnostics[0].code,
        mg_diag::codes::CURVATURE_LIMIT_EXCEEDED
    );
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
