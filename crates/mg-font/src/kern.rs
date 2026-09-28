//! Kerning (spec §12.2): the evaluated `kern` declarations as one GPOS
//! `kern` feature under script `DFLT`, built with write-fonts'
//! pair-positioning builder rather than feature-file text.
//!
//! Precedence comes from subtable order. The builder emits every
//! glyph-pair (format 1) subtable before any class (format 2) one, and a
//! format-1 subtable that covers the first glyph but lists no entry for
//! the pair does not apply, so the lookup falls through to the class
//! subtables. A glyph–glyph `kern` therefore overrides a group `kern`
//! covering the same pair.
//!
//! A `kern` with a glyph on one side and a group on the other is
//! expanded into glyph pairs, placed after the glyph–glyph ones: it is
//! more specific than a group–group `kern` and less specific than a
//! glyph–glyph one. Among expanded pairs, the earlier declaration wins.

use indexmap::IndexMap;
use read_fonts::collections::IntSet;
use write_fonts::tables::gpos::builders::{PairPosBuilder, ValueRecordBuilder};
use write_fonts::tables::gpos::{Gpos, PositionLookup, PositionLookupList};
use write_fonts::tables::layout::builders::Builder;
use write_fonts::tables::layout::{
    Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag, Script, ScriptList,
    ScriptRecord,
};
use write_fonts::tables::variations::ivs_builder::VariationStoreBuilder;
use write_fonts::types::{GlyphId16, Tag};

/// One side of a resolved `kern`, as glyph names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernSide {
    Glyph(String),
    /// The group's member glyphs.
    Group(Vec<String>),
}

/// One `kern` declaration, evaluated and rounded (spec §12.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernRule {
    pub left: KernSide,
    pub right: KernSide,
    /// Rounded per spec §10.4; range-checked by assembly.
    pub value: i64,
}

fn members(side: &KernSide) -> Vec<&str> {
    match side {
        KernSide::Glyph(name) => vec![name.as_str()],
        KernSide::Group(names) => names.iter().map(String::as_str).collect(),
    }
}

fn advance(value: i16) -> ValueRecordBuilder {
    ValueRecordBuilder::new().with_x_advance(value)
}

/// The GPOS table for `rules`, or `None` when there are none (the font
/// then has no GPOS at all). `ids` maps every glyph name to its glyph ID;
/// every value already fits int16.
pub fn build_gpos(rules: &[KernRule], ids: &IndexMap<&str, usize>) -> Option<Gpos> {
    if rules.is_empty() {
        return None;
    }
    let gid = |name: &str| GlyphId16::new(ids[name] as u16);
    let mut pairs = PairPosBuilder::default();

    // `insert_pair` keeps the first value it sees for a pair, so the
    // insertion order below is the precedence order.
    let is_glyph = |side: &KernSide| matches!(side, KernSide::Glyph(_));
    let glyph_glyph = rules
        .iter()
        .filter(|r| is_glyph(&r.left) && is_glyph(&r.right));
    let mixed = rules
        .iter()
        .filter(|r| is_glyph(&r.left) != is_glyph(&r.right));
    for rule in glyph_glyph.chain(mixed) {
        for left in members(&rule.left) {
            for right in members(&rule.right) {
                pairs.insert_pair(
                    gid(left),
                    advance(rule.value as i16),
                    gid(right),
                    ValueRecordBuilder::new(),
                );
            }
        }
    }

    for rule in rules {
        if let (KernSide::Group(left), KernSide::Group(right)) = (&rule.left, &rule.right) {
            let class = |names: &[String]| names.iter().map(|n| gid(n)).collect::<IntSet<_>>();
            pairs.insert_classes(
                class(left),
                advance(rule.value as i16),
                class(right),
                ValueRecordBuilder::new(),
            );
        }
    }

    // No variations, so the store only has to exist for the builder's
    // signature; nothing reads it.
    let mut var_store = VariationStoreBuilder::new(0);
    let subtables = pairs.build(&mut var_store);
    let lookup = PositionLookup::Pair(Lookup::new(LookupFlag::empty(), subtables));

    let script = Script::new(Some(LangSys::new(vec![0])), Vec::new());
    Some(Gpos::new(
        ScriptList::new(vec![ScriptRecord::new(Tag::new(b"DFLT"), script)]),
        FeatureList::new(vec![FeatureRecord::new(
            Tag::new(b"kern"),
            Feature::new(None, vec![0]),
        )]),
        PositionLookupList::new(vec![lookup]),
    ))
}
