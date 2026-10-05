//! Integration tests for `mg_hir::lower` (spec §5.3–§5.11, §13): the
//! conformance sample must lower cleanly, and one representative case per
//! diagnostic class (name resolution, type, field validation, path
//! structure) must actually fire with a sensible message.

use std::fs;
use std::path::Path;

use mg_syntax::ast::AstNode;

fn lower(source: &str) -> (mg_hir::Hir, Vec<mg_diag::Diagnostic>) {
    let parsed = mg_syntax::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "source failed to parse: {:?}",
        parsed.diagnostics
    );
    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax()).expect("SOURCE_FILE casts");
    mg_hir::lower(&source_file)
}

fn codes(diagnostics: &[mg_diag::Diagnostic]) -> Vec<&'static str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn conformance_sample_lowers_with_zero_diagnostics() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance.mg");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
    let (hir, diagnostics) = lower(&source);

    assert!(
        diagnostics.is_empty(),
        "expected zero diagnostics, got: {:#?}",
        diagnostics
    );

    assert_eq!(hir.font.name.as_deref(), Some("Metaglyph Sans"));
    assert_eq!(hir.font.em, Some(1000));
    assert_eq!(hir.params.len(), 6);
    assert_eq!(hir.metrics.len(), 6);
    assert_eq!(hir.instances.len(), 3);
    assert!(hir.glyphs.contains_key(&("A".to_string(), None)));
    assert!(hir.glyphs.contains_key(&("nine".to_string(), None)));
}

// ---------------------------------------------------------------------
// Name resolution (MG02xx)

#[test]
fn unresolved_identifier_suggests_a_near_miss() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let stem = 100;
        glyph A (advance: sten) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0201"), "{diagnostics:#?}");
    assert!(diagnostics[0].help.iter().any(|h| h.contains("stem")));
}

#[test]
fn duplicate_top_level_name_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let stem = 100;
        let stem = 200;
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0202"), "{diagnostics:#?}");
}

#[test]
fn glyph_scope_shadowing_top_level_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let stem = 100;
        glyph A (advance: 1) {
            let stem = 50;
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0203"), "{diagnostics:#?}");
}

#[test]
fn reserved_word_as_declaration_name_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let glyphs = 1;
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0204"), "{diagnostics:#?}");
}

#[test]
fn anchor_named_bbox_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            anchor bbox (at: (0, 0))
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0205"), "{diagnostics:#?}");
}

// ---------------------------------------------------------------------
// Type checking (MG03xx)

#[test]
fn pair_where_num_expected_is_a_type_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: (1, 2)) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0301"), "{diagnostics:#?}");
}

#[test]
fn no_such_member_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: capHeight.top) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0302"), "{diagnostics:#?}");
}

#[test]
fn mixed_tuple_is_a_type_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let bad = (1, "s");
        glyph A (advance: 1) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0303"), "{diagnostics:#?}");
}

#[test]
fn wrong_argument_count_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        let bad = sqrt(1, 2);
        glyph A (advance: 1) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0305"), "{diagnostics:#?}");
}

// ---------------------------------------------------------------------
// Field validation (MG04xx)

#[test]
fn unknown_field_suggests_a_near_miss() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advnace: 1) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0401"), "{diagnostics:#?}");
}

#[test]
fn unknown_enum_value_enumerates_the_legal_set() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10, joins: "mitre") { start (at: (0,0)) line (to: (1,0)) }
        }
    "#;
    let (_, diagnostics) = lower(src);
    let diag = diagnostics
        .iter()
        .find(|d| d.code.as_str() == "MG0402")
        .expect("expected an unknown-enum-value diagnostic");
    assert!(diag.message.contains("miter, round, bevel"));
}

#[test]
fn missing_required_field_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A () {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0403"), "{diagnostics:#?}");
}

#[test]
fn codepoint_and_glyphset_are_mutually_exclusive() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1, codepoint: 65) {}
        glyph A (advance: 1, glyphset: Alt, codepoint: 66) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0404"), "{diagnostics:#?}");
}

#[test]
fn joins_without_stroke_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (joins: "round") { start (at: (0,0)) line (to: (1,0)) }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0405"), "{diagnostics:#?}");
}

#[test]
fn non_constant_param_default_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        param stem (default: em)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0406"), "{diagnostics:#?}");
}

#[test]
fn param_default_outside_range_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        param stem (default: 500, range: 20..260)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0407"), "{diagnostics:#?}");
}

#[test]
fn codepoint_out_of_range_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (codepoint: 0x110000, advance: 1) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0411"), "{diagnostics:#?}");
}

#[test]
fn missing_required_metric_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0413"), "{diagnostics:#?}");
}

#[test]
fn nonzero_baseline_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 10, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0414"), "{diagnostics:#?}");
}

#[test]
fn alternate_glyph_without_default_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1, glyphset: Alt) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0409"), "{diagnostics:#?}");
}

#[test]
fn duplicate_kern_pair_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {}
        glyph V (advance: 1) {}
        kern (left: A, right: V, by: -10)
        kern (left: A, right: V, by: -20)
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0419"), "{diagnostics:#?}");
}

// ---------------------------------------------------------------------
// Path structure (MG05xx)

#[test]
fn path_without_body_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {}
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0501"), "{diagnostics:#?}");
}

#[test]
fn fill_requires_a_closed_path() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (fill: true) { start (at: (0,0)) line (to: (1,0)) }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0507"), "{diagnostics:#?}");
}

#[test]
fn quad_reflection_after_a_different_kind_is_illegal() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                line (to: (1,1))
                quad (to: (2,2))
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0508"), "{diagnostics:#?}");
}

#[test]
fn arc_mixing_center_and_radii_mode_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                arc (to: (10,10), center: (0,10), rx: 5, ry: 5, sweep: "cw")
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0404"), "{diagnostics:#?}");
}

#[test]
fn arc_rx_without_ry_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                arc (to: (10,10), rx: 5, sweep: "cw")
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0405"), "{diagnostics:#?}");
}

#[test]
fn arc_with_neither_center_nor_radii_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                arc (to: (10,10), sweep: "cw")
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0403"), "{diagnostics:#?}");
}

#[test]
fn arc_radii_mode_with_large_lowers_with_zero_diagnostics() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                arc (to: (10,10), rx: 5, ry: 5, large: true, sweep: "cw")
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn arc_negative_rx_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10) {
                start (at: (0,0))
                arc (to: (10,10), rx: -5, ry: 5, sweep: "cw")
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0407"), "{diagnostics:#?}");
}

fn glyph_path<'a>(hir: &'a mg_hir::Hir, glyph: &str, path: &str) -> &'a mg_hir::model::PathDecl {
    hir.glyphs[&(glyph.to_string(), None)]
        .path_named(path)
        .unwrap_or_else(|| panic!("no path named `{path}` in glyph `{glyph}`"))
}

#[test]
fn caps_bare_string_sets_both_ends() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10, caps: "round") {
                start (at: (0,0))
                line (to: (10,10))
            }
        }
    "#;
    let (hir, diagnostics) = lower(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let caps = glyph_path(&hir, "A", "p").caps.as_ref().unwrap();
    assert_eq!(caps.start, "round");
    assert_eq!(caps.end, "round");
}

#[test]
fn caps_tuple_sets_start_and_end_separately() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10, caps: ("butt", "square")) {
                start (at: (0,0))
                line (to: (10,10))
            }
        }
    "#;
    let (hir, diagnostics) = lower(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let caps = glyph_path(&hir, "A", "p").caps.as_ref().unwrap();
    assert_eq!(caps.start, "butt");
    assert_eq!(caps.end, "square");
}

#[test]
fn caps_tuple_with_wrong_arity_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10, caps: ("butt", "round", "square")) {
                start (at: (0,0))
                line (to: (10,10))
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0301"), "{diagnostics:#?}");
}

#[test]
fn caps_unknown_value_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 1) {
            path p (stroke: 10, caps: "pointy") {
                start (at: (0,0))
                line (to: (10,10))
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0402"), "{diagnostics:#?}");
}

#[test]
fn a_glyph_takes_at_most_two_of_advance_lsb_rsb() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph A (advance: 300, lsb: 10) {}
        glyph B (lsb: 10) {}
        glyph C (rsb: 10) {}
        glyph D (advance: 300, lsb: 10, rsb: 10) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert_eq!(codes(&diagnostics), vec!["MG0404"], "{diagnostics:#?}");
}

// ---------------------------------------------------------------------
// Variation sequences (spec §5.6)

const METRICS: &str = r#"
    font (name: "T", em: 1000)
    metric baseline (y: 0, align: "bottom")
    metric xHeight (y: 500)
    metric capHeight (y: 700)
    metric ascender (y: 740)
    metric descender (y: -200)
"#;

#[test]
fn variation_lowers_one_pair_or_a_list() {
    let (hir, diagnostics) = lower(&format!(
        "{METRICS}
        glyph zero_vs1 (advance: 1, variation: ('0', U+FE00)) {{}}
        glyph more (advance: 1, variation: [(U+2229, U+FE00), (U+4E00, U+E0100)]) {{}}
    "
    ));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let variations = |name: &str| hir.glyphs[&(name.to_string(), None)].variations.clone();
    assert_eq!(variations("zero_vs1"), vec![(0x30, 0xFE00)]);
    assert_eq!(variations("more"), vec![(0x2229, 0xFE00), (0x4E00, 0xE0100)]);
}

#[test]
fn variation_rejects_non_pairs_and_non_selectors() {
    let (_, diagnostics) = lower(&format!(
        "{METRICS}
        glyph a (advance: 1, variation: U+0030) {{}}
        glyph b (advance: 1, variation: ('0', 'A')) {{}}
        glyph c (advance: 1, variation: ('0', U+FE00, U+FE01)) {{}}
    "
    ));
    let found = codes(&diagnostics);
    assert!(found.contains(&"MG0423"), "{diagnostics:#?}");
    assert_eq!(found.iter().filter(|c| **c == "MG0301").count(), 2, "{diagnostics:#?}");
}

#[test]
fn an_alternate_may_not_declare_variation() {
    let (_, diagnostics) = lower(&format!(
        "{METRICS}
        glyph zero (advance: 1, codepoint: '0') {{}}
        glyph zero (advance: 1, glyphset: Alt, variation: ('0', U+FE00)) {{}}
    "
    ));
    assert!(codes(&diagnostics).contains(&"MG0410"), "{diagnostics:#?}");
}

// ---------------------------------------------------------------------
// Path components (spec §5.7)

#[test]
fn a_path_component_lowers_with_its_overrides() {
    let (hir, diagnostics) = lower(&format!(
        r#"{METRICS}
        glyph o (advance: 1) {{
            path bowl (stroke: 2) {{ start (at: (0, 0)) line (to: (0, 1)) }}
        }}
        glyph A (advance: 1) {{
            path bar (stroke: 2) {{ start (at: (0, 0)) line named (to: (1, 0)) line (to: (1, 1)) }}
            component (path: bar, stroke: 3, caps: "round", joinAt: {{ named: "bevel" }})
            component (path: glyphs.o.bowl, offset: (10, 0))
        }}
    "#
    ));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let a = &hir.glyphs[&("A".to_string(), None)];
    assert_eq!(a.components[0].source_path("A"), Some(("A".into(), "bar".into())));
    assert_eq!(a.components[0].joins, None);
    assert_eq!(a.components[0].caps.as_ref().map(|c| c.start.as_str()), Some("round"));
    assert_eq!(a.components[1].source_path("A"), Some(("o".into(), "bowl".into())));
}

#[test]
fn a_component_needs_exactly_one_of_glyph_and_path() {
    let (_, diagnostics) = lower(&format!(
        r#"{METRICS}
        glyph b (advance: 1) {{}}
        glyph A (advance: 1) {{
            path bar (stroke: 2) {{ start (at: (0, 0)) line (to: (1, 0)) }}
            component (offset: (1, 0))
            component (glyph: b, path: bar)
        }}
    "#
    ));
    let found = codes(&diagnostics);
    assert!(found.contains(&"MG0403"), "{diagnostics:#?}");
    assert!(found.contains(&"MG0404"), "{diagnostics:#?}");
}

#[test]
fn stroke_settings_are_illegal_on_a_glyph_component() {
    let (_, diagnostics) = lower(&format!(
        r#"{METRICS}
        glyph b (advance: 1) {{}}
        glyph A (advance: 1) {{ component (glyph: b, stroke: 3) }}
    "#
    ));
    assert_eq!(codes(&diagnostics), ["MG0405"], "{diagnostics:#?}");
}

#[test]
fn a_component_path_must_be_a_path_and_its_join_keys_its_segments() {
    let (_, diagnostics) = lower(&format!(
        r#"{METRICS}
        glyph A (advance: 1) {{
            let p = (1, 2);
            path bar (stroke: 2) {{ start (at: (0, 0)) line (to: (1, 0)) }}
            component (path: p)
            component (path: bar, joinAt: {{ nope: "round" }})
        }}
    "#
    ));
    let found = codes(&diagnostics);
    assert!(found.contains(&"MG0301"), "{diagnostics:#?}");
    assert!(found.contains(&"MG0201"), "{diagnostics:#?}");
}

#[test]
fn caps_on_a_closed_path_is_a_warning_on_the_caps_field() {
    let source = format!(
        r#"{METRICS}
        glyph O (advance: 1) {{
            path ring (stroke: 2, caps: "round") {{
                start (at: (0, 0))
                line (to: (1, 0))
                line (to: (1, 1))
                close
            }}
        }}
    "#
    );
    let (_, diagnostics) = lower(&source);
    assert_eq!(codes(&diagnostics), ["MG0424"], "{diagnostics:#?}");
    assert_eq!(diagnostics[0].severity, mg_diag::Severity::Warning);
    let span = diagnostics[0].primary.span.clone();
    assert_eq!(&source[span], r#"caps: "round""#);
}

#[test]
fn the_notdef_glyph_maps_no_character() {
    let (_, diagnostics) = lower(&format!(
        r#"{METRICS}
        glyph notdef (advance: 1, codepoint: U+FFFD) {{}}
    "#
    ));
    assert_eq!(codes(&diagnostics), ["MG0405"], "{diagnostics:#?}");
}
