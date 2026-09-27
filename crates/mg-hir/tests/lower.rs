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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/metaglyph-sans.mg");
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
    assert!(codes(&diagnostics).contains(&"MG0204"), "{diagnostics:#?}");
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
    assert!(codes(&diagnostics).contains(&"MG0205"), "{diagnostics:#?}");
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
    assert!(codes(&diagnostics).contains(&"MG0206"), "{diagnostics:#?}");
}

#[test]
fn case_fold_collision_between_glyph_names_is_an_error() {
    let src = r#"
        font (name: "T", em: 1000)
        metric baseline (y: 0, align: "bottom")
        metric xHeight (y: 500)
        metric capHeight (y: 700)
        metric ascender (y: 740)
        metric descender (y: -200)
        glyph aacute (advance: 1) {}
        glyph Aacute (advance: 1) {}
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0203"), "{diagnostics:#?}");
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
fn path_without_body_or_follows_is_an_error() {
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
    assert!(codes(&diagnostics).contains(&"MG0508"), "{diagnostics:#?}");
}

#[test]
fn curl_on_a_non_final_segment_is_illegal() {
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
                spline (to: (1,1), curl: 2)
                spline (to: (2,2))
            }
        }
    "#;
    let (_, diagnostics) = lower(src);
    assert!(codes(&diagnostics).contains(&"MG0405"), "{diagnostics:#?}");
}
