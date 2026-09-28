//! Integration tests for kerning (spec §12.2, plan M7): `kern`
//! declarations through `mg_font::build_fonts`, with the GPOS read back
//! by read-fonts. [`kern_value`] applies a pair-positioning lookup the
//! way a shaper does, so precedence is tested as behaviour rather than
//! as byte layout.

use mg_diag::codes;
use mg_font::{BuildOptions, BuiltFont, build_fonts};
use mg_hir::model::Hir;
use mg_syntax::ast::AstNode;
use read_fonts::tables::gpos::{PairPos, PositionLookup};
use read_fonts::types::{GlyphId, GlyphId16};
use read_fonts::{FontRef, TableProvider};
use skrifa::MetadataProvider;

const SOURCE: &str = r#"
font (name: "Kern Test", em: 1000)
metric baseline (y: 0, align: "bottom")
metric xHeight (y: 500)
metric capHeight (y: 700)
metric ascender (y: 740)
metric descender (y: -200, align: "bottom")

param k (default: 1, range: 0..2)
instance Regular ()
instance Tight (k: 2)

glyph A (codepoint: 'A', advance: 500) {}
glyph V (codepoint: 'V', advance: 500) {}
glyph O (codepoint: 'O', advance: 500) {}
glyph C (codepoint: 'C', advance: 500) {}
glyph T (codepoint: 'T', advance: 500) {}
glyph o (codepoint: 'o', advance: 500) {}
glyph e (codepoint: 'e', advance: 500) {}

group roundR (glyphs: [O, C])
group roundL (glyphs: [O, C])
group lowerRound (glyphs: [o, e])

kern (left: A, right: V, by: -0.05em * k)
kern (left: roundR, right: roundL, by: -10.4)
kern (left: O, right: C, by: 0)
kern (left: T, right: lowerRound, by: -80)
kern (left: roundR, right: T, by: -30.5)
"#;

fn lower(source: &str) -> Hir {
    let parsed = mg_syntax::parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax()).expect("SOURCE_FILE casts");
    let (hir, diagnostics) = mg_hir::lower(&source_file);
    assert!(diagnostics.is_empty(), "{:#?}", diagnostics);
    hir
}

fn build_ok(source: &str) -> Vec<BuiltFont> {
    let (fonts, diagnostics) = build_fonts(&lower(source), &BuildOptions::default());
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    fonts
}

fn font_named<'a>(fonts: &'a [BuiltFont], instance: &str) -> FontRef<'a> {
    let font = fonts.iter().find(|f| f.instance == instance).unwrap();
    FontRef::new(&font.data).unwrap()
}

fn gid(font: &FontRef, ch: char) -> GlyphId16 {
    let id: GlyphId = font.charmap().map(ch).unwrap();
    GlyphId16::new(id.to_u32() as u16)
}

/// The `kern` feature's pair adjustment for `left` then `right`, applied
/// as OpenType specifies: subtables in order, and the first whose
/// coverage holds `left` and that has an entry for the pair decides. A
/// format-1 subtable without the pair does not apply; a format-2 one
/// always does once its coverage matches.
fn kern_value(font: &FontRef, left: char, right: char) -> Option<i16> {
    let (g1, g2) = (gid(font, left), gid(font, right));
    let gpos = font.gpos().ok()?;
    let lookup = gpos.lookup_list().unwrap().lookups().get(0).unwrap();
    let PositionLookup::Pair(lookup) = lookup else {
        panic!("the kern lookup is pair positioning");
    };
    for subtable in lookup.subtables().iter() {
        match subtable.unwrap() {
            PairPos::Format1(sub) => {
                let Some(index) = sub.coverage().unwrap().get(g1) else {
                    continue;
                };
                let set = sub.pair_sets().get(index as usize).unwrap();
                for record in set.pair_value_records().iter() {
                    let record = record.unwrap();
                    if record.second_glyph() == g2 {
                        return Some(record.value_record1().x_advance().unwrap_or(0));
                    }
                }
            }
            PairPos::Format2(sub) => {
                if sub.coverage().unwrap().get(g1).is_none() {
                    continue;
                }
                let class1 = sub.class_def1().unwrap().get(g1);
                let class2 = sub.class_def2().unwrap().get(g2);
                let record = sub.class1_records().get(class1 as usize).unwrap();
                let record = record.class2_records().get(class2 as usize).unwrap();
                return Some(record.value_record1().x_advance().unwrap_or(0));
            }
        }
    }
    None
}

#[test]
fn one_kern_feature_under_dflt_with_one_pair_lookup() {
    let fonts = build_ok(SOURCE);
    let font = font_named(&fonts, "Regular");
    let gpos = font.gpos().unwrap();

    let scripts = gpos.script_list().unwrap();
    let tags: Vec<_> = scripts
        .script_records()
        .iter()
        .map(|r| r.script_tag().to_string())
        .collect();
    assert_eq!(tags, ["DFLT"]);
    let script = scripts.script_records()[0]
        .script(scripts.offset_data())
        .unwrap();
    let lang = script.default_lang_sys().unwrap().unwrap();
    assert_eq!(lang.feature_indices().len(), 1);
    assert!(script.lang_sys_records().is_empty());

    let features = gpos.feature_list().unwrap();
    let tags: Vec<_> = features
        .feature_records()
        .iter()
        .map(|r| r.feature_tag().to_string())
        .collect();
    assert_eq!(tags, ["kern"]);
    assert_eq!(gpos.lookup_list().unwrap().lookup_count(), 1);
}

#[test]
fn glyph_pairs_come_before_classes_kept_as_classes() {
    let fonts = build_ok(SOURCE);
    let font = font_named(&fonts, "Regular");
    let gpos = font.gpos().unwrap();
    let PositionLookup::Pair(lookup) = gpos.lookup_list().unwrap().lookups().get(0).unwrap() else {
        panic!("the kern lookup is pair positioning");
    };
    let formats: Vec<u16> = lookup
        .subtables()
        .iter()
        .map(|s| match s.unwrap() {
            PairPos::Format1(_) => 1,
            PairPos::Format2(_) => 2,
        })
        .collect();
    assert_eq!(formats, [1, 2], "one of each, format 1 first");
}

#[test]
fn every_kind_of_kern_applies_with_spec_precedence() {
    let fonts = build_ok(SOURCE);
    let font = font_named(&fonts, "Regular");
    // Glyph–glyph.
    assert_eq!(kern_value(&font, 'A', 'V'), Some(-50));
    assert_eq!(kern_value(&font, 'V', 'A'), None);
    // Group–group, rounded half away from zero.
    assert_eq!(kern_value(&font, 'C', 'O'), Some(-10));
    assert_eq!(kern_value(&font, 'O', 'O'), Some(-10));
    // A glyph–glyph kern overrides the group kern covering it, even at 0.
    assert_eq!(kern_value(&font, 'O', 'C'), Some(0));
    // Glyph–group and group–glyph.
    assert_eq!(kern_value(&font, 'T', 'o'), Some(-80));
    assert_eq!(kern_value(&font, 'T', 'e'), Some(-80));
    assert_eq!(kern_value(&font, 'O', 'T'), Some(-31));
    assert_eq!(kern_value(&font, 'C', 'T'), Some(-31));
    // Nothing declared.
    assert_eq!(kern_value(&font, 'T', 'O'), None);
}

#[test]
fn kern_values_vary_per_instance() {
    let fonts = build_ok(SOURCE);
    assert_eq!(
        kern_value(&font_named(&fonts, "Regular"), 'A', 'V'),
        Some(-50)
    );
    assert_eq!(
        kern_value(&font_named(&fonts, "Tight"), 'A', 'V'),
        Some(-100)
    );
}

#[test]
fn max_context_is_two_with_kerning_and_zero_without() {
    let fonts = build_ok(SOURCE);
    let font = font_named(&fonts, "Regular");
    assert_eq!(font.os2().unwrap().us_max_context(), Some(2));

    let without: String = SOURCE
        .lines()
        .filter(|l| !l.starts_with("kern"))
        .collect::<Vec<_>>()
        .join("\n");
    let fonts = build_ok(&without);
    let font = font_named(&fonts, "Regular");
    assert!(font.gpos().is_err(), "no kerns, no GPOS");
    assert_eq!(font.os2().unwrap().us_max_context(), Some(0));
}

#[test]
fn builds_with_kerning_are_byte_identical() {
    let hir = lower(SOURCE);
    let build = || {
        build_fonts(&hir, &BuildOptions::default()).0[0]
            .data
            .clone()
    };
    assert_eq!(build(), build());
}

#[test]
fn a_kern_beyond_int16_is_an_error() {
    let source = SOURCE.replace("by: -80", "by: -40000");
    let (fonts, diagnostics) = build_fonts(&lower(&source), &BuildOptions::default());
    assert!(fonts.is_empty());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].code, codes::KERN_OUT_OF_RANGE);
    assert_eq!(diagnostics[0].note, ["in instances `Regular`, `Tight`"]);
}
