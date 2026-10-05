//! Integration tests for TTF assembly (spec §10–§12, plan M6): whole
//! sources through `mg_font::build_fonts`, read back with read-fonts and
//! skrifa.

use mg_diag::codes;
use mg_font::{BuildOptions, BuiltFont, build_fonts};
use mg_hir::model::Hir;
use mg_syntax::ast::AstNode;
use read_fonts::tables::glyf::Glyph;
use read_fonts::types::{GlyphId, GlyphId16};
use read_fonts::{FontRef, TableProvider};
use skrifa::MetadataProvider;
use skrifa::string::StringId;

const PREAMBLE: &str = r#"
font (name: "Test Sans", em: 1000, version: "2.500", designer: "D. Signer")
metric baseline (y: 0, overshoot: 10, align: "bottom")
metric xHeight (y: 500)
metric capHeight (y: 700, overshoot: 12)
metric ascender (y: 740)
metric descender (y: -200, align: "bottom")
"#;

const GLYPHS: &str = r#"
glyph I (codepoint: 'I', advance: 200) {
  path stem (stroke: 80) {
    start (at: (100, 0))
    line (to: (100, 700))
  }
}

glyph O (codepoint: 'O', advance: 600) {
  path bowl (stroke: 80) {
    start (at: (300, 40))
    arc (center: (300, 350), to: (300, 660), sweep: "ccw")
    arc (center: (300, 350), to: (300, 40), sweep: "ccw")
    close
  }
}

glyph Iacute (codepoint: U+00CD, advance: glyphs.I.advance) {
  component (glyph: I)
}

glyph Irot (codepoint: U+1F600, advance: 200) {
  component (glyph: I, transform: (rotate(90deg), translate(700, 0)))
}
"#;

fn lower(source: &str) -> Hir {
    let parsed = mg_syntax::parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax()).expect("SOURCE_FILE casts");
    let (hir, diagnostics) = mg_hir::lower(&source_file);
    assert!(diagnostics.is_empty(), "{:#?}", diagnostics);
    hir
}

fn build(source: &str) -> (Vec<BuiltFont>, Vec<mg_diag::Diagnostic>) {
    build_fonts(&lower(source), &BuildOptions::default())
}

fn build_ok(source: &str) -> Vec<BuiltFont> {
    let (fonts, diagnostics) = build(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    fonts
}

fn font_named<'a>(fonts: &'a [BuiltFont], instance: &str) -> FontRef<'a> {
    let font = fonts.iter().find(|f| f.instance == instance).unwrap();
    FontRef::new(&font.data).unwrap()
}

fn name(font: &FontRef, id: StringId) -> Option<String> {
    font.localized_strings(id)
        .english_or_first()
        .map(|s| s.to_string())
}

fn glyph<'a>(font: &FontRef<'a>, gid: u32) -> Option<Glyph<'a>> {
    font.loca(None)
        .unwrap()
        .get_glyf(GlyphId::new(gid), &font.glyf().unwrap())
        .unwrap()
}

#[test]
fn every_instance_builds_with_its_own_file_name() {
    let source = format!(
        "{PREAMBLE}
instance Regular ()
instance Bold (weightClass: 700)
instance WideItalic (styleName: \"Wide Italic\", slant: 10deg, widthClass: 7)
{GLYPHS}"
    );
    let fonts = build_ok(&source);
    let names: Vec<&str> = fonts.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "TestSans-Regular.ttf",
            "TestSans-Bold.ttf",
            "TestSans-WideItalic.ttf"
        ]
    );
}

#[test]
fn glyph_order_cmap_and_metrics() {
    let fonts = build_ok(&format!("{PREAMBLE}{GLYPHS}"));
    assert_eq!(fonts.len(), 1, "the implicit Regular instance");
    let font = font_named(&fonts, "Regular");

    let post = font.post().unwrap();
    let order: Vec<&str> = (0..5)
        .map(|i| post.glyph_name(GlyphId16::new(i)).unwrap())
        .collect();
    assert_eq!(order, [".notdef", "I", "O", "Iacute", "Irot"]);

    let charmap = font.charmap();
    assert_eq!(charmap.map('I'), Some(GlyphId::new(1)));
    assert_eq!(charmap.map('\u{CD}'), Some(GlyphId::new(3)));
    assert_eq!(charmap.map('\u{1F600}'), Some(GlyphId::new(4)));
    // Format 4 for the BMP, and format 12 since U+1F600 is beyond it.
    let formats: Vec<(u16, u16)> = font
        .cmap()
        .unwrap()
        .encoding_records()
        .iter()
        .map(|r| (r.platform_id() as u16, r.encoding_id()))
        .collect();
    assert_eq!(formats, [(0, 3), (0, 4), (3, 1), (3, 10)]);

    // `.notdef` is empty at half an em; the rest take their `advance`.
    let hmtx = font.hmtx().unwrap();
    let advances: Vec<u16> = hmtx.h_metrics().iter().map(|m| m.advance()).collect();
    assert_eq!(advances, [500, 200, 600, 200, 200]);
    assert!(glyph(&font, 0).is_none());
    // Left sidebearings are the final outline's xMin.
    let lsbs: Vec<i16> = hmtx.h_metrics().iter().map(|m| m.side_bearing()).collect();
    assert_eq!(lsbs[1], 60);
    assert_eq!(lsbs[4], 0);

    let hhea = font.hhea().unwrap();
    assert_eq!(hhea.ascender().to_i16(), 740);
    assert_eq!(hhea.descender().to_i16(), -200);
    assert_eq!(hhea.line_gap().to_i16(), 0);
    let os2 = font.os2().unwrap();
    assert_eq!(os2.s_typo_ascender(), 740);
    assert_eq!(os2.s_cap_height(), Some(700));
    assert_eq!(os2.sx_height(), Some(500));
    assert_eq!(os2.y_strikeout_position(), 250);
    assert_eq!(os2.us_win_ascent(), 700);

    let post = font.post().unwrap();
    assert_eq!(post.underline_position().to_i16(), -100);
    assert_eq!(post.underline_thickness().to_i16(), 50);

    // spec §11.4: one range, flags 0x000F.
    let gasp = font.gasp().unwrap();
    let range = &gasp.gasp_ranges()[0];
    assert_eq!(range.range_max_ppem(), 0xFFFF);
    assert_eq!(range.range_gasp_behavior().bits(), 0x000F);
}

#[test]
fn every_simple_glyph_is_flagged_overlapping() {
    let fonts = build_ok(&format!("{PREAMBLE}{GLYPHS}"));
    let font = font_named(&fonts, "Regular");
    for gid in [1, 2] {
        let Some(Glyph::Simple(simple)) = glyph(&font, gid) else {
            panic!("glyph {gid} is simple");
        };
        assert!(simple.has_overlapping_contours(), "glyph {gid}");
    }
    let Some(Glyph::Composite(composite)) = glyph(&font, 3) else {
        panic!("Iacute stays composite");
    };
    let first = composite.components().next().unwrap();
    assert!(
        first
            .flags
            .contains(read_fonts::tables::glyf::CompositeGlyphFlags::OVERLAP_COMPOUND)
    );
}

#[test]
fn components_stay_composite_or_decompose_per_spec_10_1() {
    let source = format!(
        "{PREAMBLE}{GLYPHS}
glyph Ibig (advance: 700) {{
  // A scale of 3 does not fit F2Dot14.
  component (glyph: I, transform: scale(3))
}}

glyph Iplus (advance: 400) {{
  // Contours and a component: `glyf` cannot mix them.
  path bar (stroke: 40) {{
    start (at: (0, 350))
    line (to: (300, 350))
  }}
  component (glyph: I, offset: (100, 0))
}}
"
    );
    let fonts = build_ok(&source);
    let font = font_named(&fonts, "Regular");

    let Some(Glyph::Composite(rotated)) = glyph(&font, 4) else {
        panic!("Irot stays composite");
    };
    let component = rotated.components().next().unwrap();
    assert_eq!(component.glyph, GlyphId16::new(1));

    let Some(Glyph::Simple(big)) = glyph(&font, 5) else {
        panic!("Ibig is decomposed");
    };
    assert_eq!(big.number_of_contours(), 1);
    // The stem, three times the size: x 180..420, y 0..2100.
    assert_eq!(
        (big.x_min(), big.y_min(), big.x_max(), big.y_max()),
        (180, 0, 420, 2100)
    );

    let Some(Glyph::Simple(plus)) = glyph(&font, 6) else {
        panic!("Iplus is decomposed");
    };
    assert_eq!(plus.number_of_contours(), 2);
    assert_eq!((plus.x_min(), plus.x_max()), (0, 300));

    // The rotated composite's bounds are the decomposed outline's.
    let bounds = font
        .glyph_metrics(
            skrifa::instance::Size::unscaled(),
            skrifa::instance::LocationRef::default(),
        )
        .bounds(GlyphId::new(4))
        .unwrap();
    assert_eq!(
        (bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max),
        (0.0, 60.0, 700.0, 140.0)
    );
}

#[test]
fn naming_and_style_bits_follow_spec_12_3() {
    let source = format!(
        "{PREAMBLE}
instance Regular ()
instance Bold (weightClass: 700)
instance Italic (slant: 10deg)
instance Wide (widthClass: 7)
{GLYPHS}"
    );
    let fonts = build_ok(&source);

    let regular = font_named(&fonts, "Regular");
    assert_eq!(
        name(&regular, StringId::FAMILY_NAME).as_deref(),
        Some("Test Sans")
    );
    assert_eq!(
        name(&regular, StringId::SUBFAMILY_NAME).as_deref(),
        Some("Regular")
    );
    assert_eq!(
        name(&regular, StringId::FULL_NAME).as_deref(),
        Some("Test Sans Regular")
    );
    assert_eq!(
        name(&regular, StringId::POSTSCRIPT_NAME).as_deref(),
        Some("TestSans-Regular")
    );
    assert_eq!(
        name(&regular, StringId::VERSION_STRING).as_deref(),
        Some("Version 2.500")
    );
    assert_eq!(
        name(&regular, StringId::DESIGNER).as_deref(),
        Some("D. Signer")
    );
    assert_eq!(name(&regular, StringId::TYPOGRAPHIC_FAMILY_NAME), None);
    assert_eq!(regular.head().unwrap().font_revision().to_f64(), 2.5);
    // REGULAR | USE_TYPO_METRICS.
    assert_eq!(regular.os2().unwrap().fs_selection().bits(), 0x00C0);

    let bold = font_named(&fonts, "Bold");
    assert_eq!(bold.os2().unwrap().fs_selection().bits(), 0x00A0);
    assert_eq!(bold.head().unwrap().mac_style().bits(), 0x0001);
    assert_eq!(bold.os2().unwrap().us_weight_class(), 700);

    let italic = font_named(&fonts, "Italic");
    assert_eq!(italic.os2().unwrap().fs_selection().bits(), 0x0081);
    assert_eq!(italic.head().unwrap().mac_style().bits(), 0x0002);
    assert_eq!(italic.post().unwrap().italic_angle().to_f64(), -10.0);
    let hhea = italic.hhea().unwrap();
    assert_eq!(hhea.caret_slope_rise(), 1000);
    assert_eq!(hhea.caret_slope_run(), 176);

    let wide = font_named(&fonts, "Wide");
    assert_eq!(
        name(&wide, StringId::FAMILY_NAME).as_deref(),
        Some("Test Sans Wide")
    );
    assert_eq!(
        name(&wide, StringId::SUBFAMILY_NAME).as_deref(),
        Some("Regular")
    );
    assert_eq!(
        name(&wide, StringId::TYPOGRAPHIC_FAMILY_NAME).as_deref(),
        Some("Test Sans")
    );
    assert_eq!(
        name(&wide, StringId::TYPOGRAPHIC_SUBFAMILY_NAME).as_deref(),
        Some("Wide")
    );
    assert_eq!(wide.os2().unwrap().fs_selection().bits(), 0x00C0);
    assert_eq!(wide.os2().unwrap().us_width_class(), 7);
}

#[test]
fn builds_are_byte_identical_and_the_timestamp_is_settable() {
    let hir = lower(&format!("{PREAMBLE}{GLYPHS}"));
    let at = |timestamp| {
        build_fonts(&hir, &BuildOptions { timestamp }).0[0]
            .data
            .clone()
    };
    assert_eq!(at(0), at(0));

    let later = at(1_700_000_000);
    let font = FontRef::new(&later).unwrap();
    let head = font.head().unwrap();
    assert_eq!(head.created().as_secs(), 1_700_000_000 + 2_082_844_800);
    assert_eq!(head.modified(), head.created());
}

fn only_error(source: &str) -> mg_diag::Diagnostic {
    let (fonts, diagnostics) = build(source);
    assert!(fonts.is_empty(), "a failed build produces no font");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    diagnostics.into_iter().next().unwrap()
}

#[test]
fn a_codepoint_on_two_glyphs_is_an_error() {
    let error = only_error(&format!(
        "{PREAMBLE}{GLYPHS}
glyph I2 (codepoint: 'I', advance: 200) {{}}
"
    ));
    assert_eq!(error.code, codes::DUPLICATE_CODEPOINT);
    assert!(error.message.contains("`I` and `I2`"), "{}", error.message);
}

#[test]
fn surrogates_and_noncharacters_are_errors() {
    for cp in ["U+D800", "U+FDD0", "U+FFFE", "U+1FFFF"] {
        let error = only_error(&format!(
            "{PREAMBLE}
glyph x (codepoint: {cp}, advance: 200) {{}}
"
        ));
        assert_eq!(error.code, codes::UNENCODABLE_CODEPOINT, "{cp}");
    }
}

#[test]
fn a_coordinate_beyond_int16_is_an_error() {
    let error = only_error(&format!(
        "{PREAMBLE}
glyph wide (advance: 200) {{
  path p (stroke: 80) {{
    start (at: (0, 0))
    line (to: (40000, 0))
  }}
}}
"
    ));
    assert_eq!(error.code, codes::COORDINATE_OUT_OF_RANGE);
}

#[test]
fn a_negative_advance_is_an_error() {
    let error = only_error(&format!(
        "{PREAMBLE}
glyph back (advance: -10) {{}}
"
    ));
    assert_eq!(error.code, codes::ADVANCE_OUT_OF_RANGE);
}

#[test]
fn components_nested_six_deep_are_an_error() {
    let mut source = format!("{PREAMBLE}{GLYPHS}");
    let mut previous = "I".to_string();
    for level in 1..=6 {
        let name = format!("n{level}");
        source.push_str(&format!(
            "glyph {name} (advance: 200) {{ component (glyph: {previous}) }}\n"
        ));
        previous = name;
    }
    let (fonts, diagnostics) = build(&source);
    assert!(fonts.is_empty());
    let deep: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::COMPONENT_TOO_DEEP)
        .collect();
    assert_eq!(deep.len(), 1, "{diagnostics:#?}");
    assert!(deep[0].message.contains("`n6`"));
}

#[test]
fn an_error_in_one_instance_is_reported_once_naming_it() {
    let source = format!(
        "{PREAMBLE}
param w (default: 200, range: -100..300)
instance Regular ()
instance Broken (w: -10)
glyph x (advance: w) {{}}
"
    );
    let error = only_error(&source);
    assert_eq!(error.code, codes::ADVANCE_OUT_OF_RANGE);
    assert_eq!(error.note, ["in instance `Broken`"]);
}

const ZERO: &str = r#"
glyph zero (codepoint: '0', advance: 600) {
  path bowl (stroke: 80) {
    start (at: (300, 40))
    line (to: (300, 660))
  }
}
"#;

/// The font's `cmap` format 14 subtable and its encoding record.
fn format_14<'a>(
    font: &FontRef<'a>,
) -> (
    (read_fonts::tables::cmap::PlatformId, u16),
    read_fonts::tables::cmap::Cmap14<'a>,
) {
    use read_fonts::tables::cmap::CmapSubtable;
    let cmap = font.cmap().unwrap();
    cmap.encoding_records()
        .iter()
        .find_map(|r| match r.subtable(cmap.offset_data()).ok()? {
            CmapSubtable::Format14(t) => Some(((r.platform_id(), r.encoding_id()), t)),
            _ => None,
        })
        .expect("a format 14 subtable")
}

#[test]
fn variation_sequences_build_cmap_format_14() {
    use read_fonts::tables::cmap::{MapVariant, PlatformId};
    let fonts = build_ok(&format!(
        "{PREAMBLE}{GLYPHS}{ZERO}
glyph zero_vs1 (variation: ('0', U+FE00), advance: 600) {{}}
glyph zero_text (variation: ('0', U+FE0E), advance: 600) {{}}
glyph I_text (codepoint: U+2139, variation: (U+2139, U+FE0E), advance: 200) {{}}
"
    ));
    let font = font_named(&fonts, "Regular");
    let gid = |name: &str| {
        let order = ["I", "O", "Iacute", "Irot", "zero", "zero_vs1", "zero_text", "I_text"];
        GlyphId::new(1 + order.iter().position(|n| *n == name).unwrap() as u32)
    };

    let (record, cmap14) = format_14(&font);
    assert_eq!(record, (PlatformId::Unicode, 5));
    assert_eq!(cmap14.map_variant(0x30u32, 0xFE00u32), Some(MapVariant::Variant(gid("zero_vs1"))));
    assert_eq!(cmap14.map_variant(0x30u32, 0xFE0Eu32), Some(MapVariant::Variant(gid("zero_text"))));
    // A sequence on the glyph its base encodes is a default-UVS mapping.
    assert_eq!(cmap14.map_variant(0x2139u32, 0xFE0Eu32), Some(MapVariant::UseDefault));
    assert_eq!(cmap14.map_variant(0x31u32, 0xFE00u32), None);
    // Selector records are sorted.
    let selectors: Vec<u32> = cmap14.var_selector().iter().map(|r| r.var_selector().to_u32()).collect();
    assert_eq!(selectors, vec![0xFE00, 0xFE0E]);

    // The splice left the other subtables intact.
    let cmap = font.cmap().unwrap();
    assert_eq!(cmap.map_codepoint('0'), Some(gid("zero")));
    assert_eq!(cmap.map_codepoint('\u{1F600}'), Some(gid("Irot")));
    let records: Vec<(u16, u16)> = cmap
        .encoding_records()
        .iter()
        .map(|r| (r.platform_id() as u16, r.encoding_id()))
        .collect();
    assert_eq!(records, vec![(0, 3), (0, 4), (0, 5), (3, 1), (3, 10)]);
}

#[test]
fn no_variation_sequences_means_no_format_14() {
    let fonts = build_ok(&format!("{PREAMBLE}{GLYPHS}"));
    let cmap = font_named(&fonts, "Regular").cmap().unwrap();
    assert!(cmap.encoding_records().iter().all(|r| r.encoding_id() != 5 || r.platform_id() as u16 != 0));
}

#[test]
fn a_variation_sequence_on_two_glyphs_is_an_error() {
    let error = only_error(&format!(
        "{PREAMBLE}{ZERO}
glyph a (variation: ('0', U+FE00), advance: 600) {{}}
glyph b (variation: [('0', U+FE00)], advance: 600) {{}}
"
    ));
    assert_eq!(error.code, codes::DUPLICATE_VARIATION_SEQUENCE);
    assert!(error.message.contains("`a` and `b`"), "{}", error.message);
}

#[test]
fn variation_sequence_warnings_still_build() {
    let (fonts, diagnostics) = build(&format!(
        "{PREAMBLE}{ZERO}
glyph odd (variation: ('0', U+FE05), advance: 600) {{}}
glyph lone (variation: ('Q', U+E0100), advance: 600) {{}}
"
    ));
    assert!(!fonts.is_empty(), "warnings don't stop a build");
    let codes_found: Vec<_> = diagnostics.iter().map(|d| (d.code, d.severity)).collect();
    assert_eq!(
        codes_found,
        vec![
            (codes::UNSTANDARDIZED_VARIATION_SEQUENCE, mg_diag::Severity::Warning),
            (codes::VARIATION_BASE_NOT_ENCODED, mg_diag::Severity::Warning),
        ]
    );
}

#[test]
fn a_declared_notdef_glyph_is_glyph_zero() {
    let fonts = build_ok(&format!(
        "{PREAMBLE}
glyph notdef (advance: 450) {{
  path box (stroke: 40) {{
    start (at: (50, 0))
    line (to: (400, 0))
    line (to: (400, 700))
    line (to: (50, 700))
    close
  }}
}}
glyph I (codepoint: 'I', advance: 200) {{
  path stem (stroke: 80) {{ start (at: (100, 0)) line (to: (100, 700)) }}
}}
glyph boxed (advance: 450) {{ component (glyph: notdef) }}
"
    ));
    let font = font_named(&fonts, "Regular");
    let names: Vec<String> = (0..font.maxp().unwrap().num_glyphs())
        .map(|i| {
            font.post()
                .unwrap()
                .glyph_name(GlyphId16::new(i))
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(names, [".notdef", "I", "boxed"]);
    assert_eq!(font.hmtx().unwrap().advance(GlyphId::new(0)), Some(450));
    assert!(matches!(glyph(&font, 0), Some(Glyph::Simple(_))), "it has the box's outline");
    // The component of `notdef` points at glyph 0.
    let Some(Glyph::Composite(boxed)) = glyph(&font, 2) else {
        panic!("`boxed` is a composite");
    };
    assert_eq!(boxed.components().next().unwrap().glyph, GlyphId16::new(0));
}
