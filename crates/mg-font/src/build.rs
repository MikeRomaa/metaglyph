//! `mg build` (spec §12.3): one independent build per instance —
//! evaluate, prepare every outline, assemble the tables — plus the
//! export checks that need the source to report against (spec §10.5,
//! §10.6). A build with any error produces no font at all (spec §4.6).

use std::ops::Range;

use indexmap::IndexMap;
use mg_diag::{Diagnostic, Label, Severity, codes};
use mg_eval::NodeId;
use mg_hir::model::{Hir, InstanceDecl, KernSide as HirKernSide};
use mg_syntax::ast::AstNode;

use crate::assemble::{self, FontInfo, GlyphRecord, LimitError};
use crate::kern::{KernRule, KernSide};
use crate::prepare::{self, PreparedGlyph};

#[derive(Debug, Clone, Copy, Default)]
pub struct BuildOptions {
    /// `head.created` and `head.modified`, in seconds since the Unix
    /// epoch (spec §14: settable to a fixed value, so a build can be
    /// byte-identical).
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct BuiltFont {
    pub instance: String,
    /// `font.name` without spaces, `-`, `styleName` without spaces,
    /// `.ttf`.
    pub file_name: String,
    pub data: Vec<u8>,
}

/// A diagnostic's code, message, and primary span.
type DiagnosticKey = (String, String, Range<usize>);

/// Every instance of the font, built. Nothing is returned unless every
/// instance built without error; warnings come back either way.
/// Diagnostics repeated across instances are reported once, with a note
/// naming the instances they occurred in.
pub fn build_fonts(hir: &Hir, options: &BuildOptions) -> (Vec<BuiltFont>, Vec<Diagnostic>) {
    let mut diagnostics = check_codepoints(hir);
    let codepoints_ok = !has_errors(&diagnostics);

    // Keyed by what the diagnostic says and where, so the same problem in
    // several instances is one entry listing them all.
    let mut per_instance: IndexMap<DiagnosticKey, (Diagnostic, Vec<String>)> = IndexMap::new();
    let mut fonts = Vec::new();
    for instance in hir.instances.values() {
        let (compiled, mut instance_diagnostics) = compile_instance(hir, instance, options);
        // Assembly assumes every codepoint is valid and unique, so it only
        // runs once that is known.
        if let Some((info, records, kerns)) = compiled
            && codepoints_ok
        {
            match assemble::assemble(&info, &records, &kerns) {
                Ok(data) => fonts.push(BuiltFont {
                    instance: instance.name.clone(),
                    file_name: format!(
                        "{}-{}.ttf",
                        info.family.replace(' ', ""),
                        info.style.replace(' ', "")
                    ),
                    data,
                }),
                Err(errors) => instance_diagnostics.extend(
                    errors
                        .into_iter()
                        .map(|e| limit_diagnostic(hir, instance, &records, e)),
                ),
            }
        }
        for diagnostic in instance_diagnostics {
            let key = (
                diagnostic.code.as_str().to_string(),
                diagnostic.message.clone(),
                diagnostic.primary.span.clone(),
            );
            per_instance
                .entry(key)
                .or_insert_with(|| (diagnostic, Vec::new()))
                .1
                .push(instance.name.clone());
        }
    }
    for (diagnostic, instances) in per_instance.into_values() {
        let list = instances
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let noun = if instances.len() == 1 {
            "instance"
        } else {
            "instances"
        };
        diagnostics.push(diagnostic.with_note(format!("in {noun} {list}")));
    }

    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        fonts.clear();
    }
    (fonts, diagnostics)
}

/// What assembly needs for one instance.
type Compiled = (FontInfo, Vec<GlyphRecord>, Vec<KernRule>);

/// One instance evaluated and prepared, ready for assembly (`None` on any
/// error), with its diagnostics.
fn compile_instance(
    hir: &Hir,
    instance: &InstanceDecl,
    options: &BuildOptions,
) -> (Option<Compiled>, Vec<Diagnostic>) {
    let (_, outcome) = mg_eval::evaluate(hir, instance);
    if has_errors(&outcome.diagnostics) {
        return (None, outcome.diagnostics);
    }
    let mut diagnostics = outcome.diagnostics.clone();

    let (prepared, prepare_diagnostics) = prepare::prepare_font(hir, instance, &outcome);
    diagnostics.extend(prepare_diagnostics);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let em = hir.font.em.expect("mg-hir requires `font.em`");
    let records = glyph_records(hir, &outcome, em, prepared);

    let zone_y = |name: &str| {
        outcome
            .values
            .get(&NodeId::TopLevel(name.to_string()))
            .and_then(|v| v.as_zone())
            .map_or(0.0, |z| z.y)
    };
    let info = FontInfo {
        em: em as u16,
        family: hir.font.name.clone().expect("mg-hir requires `font.name`"),
        style: instance.style_name.clone(),
        version: hir.font.version.clone(),
        designer: hir.font.designer.clone(),
        foundry: hir.font.foundry.clone(),
        license: hir.font.license.clone(),
        weight_class: instance.weight_class as u16,
        width_class: instance.width_class as u16,
        slant: prepare::instance_slant(hir, instance),
        ascender: zone_y("ascender"),
        descender: zone_y("descender"),
        cap_height: zone_y("capHeight"),
        x_height: zone_y("xHeight"),
        timestamp: options.timestamp,
    };

    let kerns = kern_rules(hir, &outcome);
    (Some((info, records, kerns)), diagnostics)
}

/// Every `kern` in declaration order (the index `LimitError` reports),
/// with its `by` evaluated for this instance and rounded (spec §12.2).
fn kern_rules(hir: &Hir, outcome: &mg_eval::EvalOutcome) -> Vec<KernRule> {
    let side = |side: &Option<HirKernSide>| match side
        .as_ref()
        .expect("mg-hir resolved every kern side")
    {
        HirKernSide::Glyph(name) => KernSide::Glyph(name.clone()),
        HirKernSide::Group(name) => KernSide::Group(hir.groups[name].glyphs.clone()),
    };
    hir.kerns
        .iter()
        .enumerate()
        .map(|(i, kern)| {
            let value = outcome
                .values
                .get(&NodeId::Kern(i))
                .and_then(|v| v.as_num())
                .expect("an error-free evaluation has every kern value");
            KernRule {
                left: side(&kern.left),
                right: side(&kern.right),
                value: value.round() as i64,
            }
        })
        .collect()
}

fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

/// `.notdef`, then every other default-set glyph in declaration order
/// (spec §10.6). `.notdef` is the declared `notdef` glyph when there is
/// one, else generated: no contours, advance `round(font.em / 2)`.
fn glyph_records(
    hir: &Hir,
    outcome: &mg_eval::EvalOutcome,
    em: i64,
    mut prepared: IndexMap<String, PreparedGlyph>,
) -> Vec<GlyphRecord> {
    let advance_of = |name: &str| {
        outcome
            .values
            .get(&NodeId::GlyphAdvance(name.to_string()))
            .and_then(|v| v.as_num())
            .expect("an error-free evaluation has every advance")
            .round() as i64
    };
    let notdef = hir.glyphs.contains_key(&(NOTDEF.to_string(), None));
    let mut records = vec![GlyphRecord {
        name: ".notdef".to_string(),
        codepoints: Vec::new(),
        variations: Vec::new(),
        advance: if notdef {
            advance_of(NOTDEF)
        } else {
            (em as f64 / 2.0).round() as i64
        },
        glyph: if notdef {
            prepared.swap_remove(NOTDEF).unwrap_or_default()
        } else {
            PreparedGlyph::default()
        },
    }];
    for ((name, glyphset), decl) in &hir.glyphs {
        if glyphset.is_some() || name == NOTDEF {
            continue;
        }
        let mut codepoints = decl.codepoints.clone();
        codepoints.dedup();
        records.push(GlyphRecord {
            name: name.clone(),
            codepoints,
            variations: decl.variations.clone(),
            advance: advance_of(name),
            glyph: prepared.swap_remove(name).unwrap_or_default(),
        });
    }
    records
}

/// The glyph declared to stand in for missing characters (spec §10.6).
const NOTDEF: &str = "notdef";

fn glyph_span(
    hir: &Hir,
    instance: &InstanceDecl,
    records: &[GlyphRecord],
    index: usize,
) -> Range<usize> {
    let name = if index == 0 { NOTDEF } else { records[index].name.as_str() };
    let Some(decl) = mg_eval::graph::effective_glyph(hir, instance, name) else {
        // A generated `.notdef`; the font declaration is the nearest thing
        // to blame.
        return mg_syntax::trimmed_range(&hir.font.syntax);
    };
    mg_syntax::trimmed_range(&decl.syntax)
}

fn limit_diagnostic(
    hir: &Hir,
    instance: &InstanceDecl,
    records: &[GlyphRecord],
    error: LimitError,
) -> Diagnostic {
    let span_of = |i| glyph_span(hir, instance, records, i);
    let name_of = |i: usize| records[i].name.clone();
    match error {
        LimitError::CoordinateOutOfRange { glyph } => Diagnostic::error(
            codes::COORDINATE_OUT_OF_RANGE,
            format!(
                "glyph `{}` has a coordinate outside the int16 range -32768..=32767",
                name_of(glyph)
            ),
            Label::new(span_of(glyph), "in this glyph"),
        ),
        LimitError::TooManyPoints { glyph, points } => Diagnostic::error(
            codes::GLYPH_LIMIT_EXCEEDED,
            format!(
                "glyph `{}` has {points} points; `glyf` allows at most 65535",
                name_of(glyph)
            ),
            Label::new(span_of(glyph), "in this glyph"),
        ),
        LimitError::TooManyContours { glyph, contours } => Diagnostic::error(
            codes::GLYPH_LIMIT_EXCEEDED,
            format!(
                "glyph `{}` has {contours} contours; `glyf` allows at most 65535",
                name_of(glyph)
            ),
            Label::new(span_of(glyph), "in this glyph"),
        ),
        LimitError::AdvanceOutOfRange { glyph, advance } => Diagnostic::error(
            codes::ADVANCE_OUT_OF_RANGE,
            format!(
                "glyph `{}` has advance {advance}; `hmtx` needs 0..=65535",
                name_of(glyph)
            ),
            Label::new(span_of(glyph), "in this glyph"),
        ),
        LimitError::KernOutOfRange { kern, value } => Diagnostic::error(
            codes::KERN_OUT_OF_RANGE,
            format!("this `kern` rounds to {value}; GPOS needs -32768..=32767"),
            Label::new(
                mg_syntax::trimmed_range(&hir.kerns[kern].syntax),
                "this kern",
            ),
        ),
        LimitError::TooManyGlyphs { count } => Diagnostic::error(
            codes::GLYPH_LIMIT_EXCEEDED,
            format!("the font has {count} glyphs, including `.notdef`; at most 65535 fit"),
            Label::new(mg_syntax::trimmed_range(&hir.font.syntax), "in this font"),
        ),
    }
}

fn unencodable(cp: u32) -> Option<&'static str> {
    if (0xD800..=0xDFFF).contains(&cp) {
        Some("a surrogate")
    } else if (0xFDD0..=0xFDEF).contains(&cp) || cp & 0xFFFE == 0xFFFE {
        Some("a noncharacter")
    } else {
        None
    }
}

/// Spec §10.6's codepoint and variation-sequence rules, which hold for
/// every instance alike: a codepoint or sequence on more than one glyph,
/// or a surrogate or noncharacter, is an error; a VS1–VS16 sequence
/// Unicode does not standardize, or one whose base no glyph encodes, is a
/// warning.
pub fn check_codepoints(hir: &Hir) -> Vec<Diagnostic> {
    let mut diagnostics = check_single_codepoints(hir);
    diagnostics.extend(check_variations(hir));
    diagnostics
}

fn check_variations(hir: &Hir) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let encoded: std::collections::HashSet<u32> = hir
        .glyphs
        .iter()
        .filter(|((_, set), _)| set.is_none())
        .flat_map(|(_, decl)| decl.codepoints.iter().copied())
        .collect();
    let mut first_use: IndexMap<(u32, u32), (&str, Range<usize>)> = IndexMap::new();

    for ((name, glyphset), decl) in &hir.glyphs {
        if glyphset.is_some() {
            continue;
        }
        let Some(expr) = &decl.variation_expr else {
            continue;
        };
        let span: Range<usize> = expr.syntax().text_range().into();
        let mut seen_here = Vec::new();
        for &(base, selector) in &decl.variations {
            if seen_here.contains(&(base, selector)) {
                continue;
            }
            seen_here.push((base, selector));
            let sequence = format!("<U+{base:04X}, U+{selector:04X}>");

            if let Some(kind) = unencodable(base) {
                diagnostics.push(Diagnostic::error(
                    codes::UNENCODABLE_CODEPOINT,
                    format!("the base of {sequence} is {kind}, which `cmap` cannot map"),
                    Label::new(span.clone(), "in this sequence"),
                ));
                continue;
            }
            if let Some((other, other_span)) = first_use.get(&(base, selector)) {
                diagnostics.push(
                    Diagnostic::error(
                        codes::DUPLICATE_VARIATION_SEQUENCE,
                        format!("{sequence} is mapped by both `{other}` and `{name}`"),
                        Label::new(span.clone(), format!("`{name}` maps it here")),
                    )
                    .with_secondary(Label::new(
                        other_span.clone(),
                        format!("`{other}` maps it here"),
                    ))
                    .with_help("a sequence maps to one glyph; remove it from one of them"),
                );
                continue;
            }
            first_use.insert((base, selector), (name.as_str(), span.clone()));

            if selector <= 0xFE0F
                && crate::sequences::STANDARDIZED
                    .binary_search(&(base, selector))
                    .is_err()
            {
                diagnostics.push(
                    Diagnostic::warning(
                        codes::UNSTANDARDIZED_VARIATION_SEQUENCE,
                        format!("{sequence} is not a standardized variation sequence in Unicode 16.0"),
                        Label::new(span.clone(), "not standardized"),
                    )
                    .with_help("text using it may not reach this glyph in other fonts or tools"),
                );
            }
            if !encoded.contains(&base) {
                diagnostics.push(
                    Diagnostic::warning(
                        codes::VARIATION_BASE_NOT_ENCODED,
                        format!("no glyph has `codepoint: U+{base:04X}`, the base of {sequence}"),
                        Label::new(span.clone(), "base not encoded"),
                    )
                    .with_help("text with the base alone falls back to another font"),
                );
            }
        }
    }
    diagnostics
}

fn check_single_codepoints(hir: &Hir) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut first_use: IndexMap<u32, (&str, Range<usize>)> = IndexMap::new();

    for ((name, glyphset), decl) in &hir.glyphs {
        if glyphset.is_some() {
            continue;
        }
        let Some(expr) = &decl.codepoint_expr else {
            continue;
        };
        let span: Range<usize> = expr.syntax().text_range().into();
        let mut seen_here = Vec::new();
        for &cp in &decl.codepoints {
            if seen_here.contains(&cp) {
                continue;
            }
            seen_here.push(cp);

            if let Some(kind) = unencodable(cp) {
                diagnostics.push(Diagnostic::error(
                    codes::UNENCODABLE_CODEPOINT,
                    format!("U+{cp:04X} is {kind}, which `cmap` cannot map"),
                    Label::new(span.clone(), "in this codepoint"),
                ));
                continue;
            }

            match first_use.get(&cp) {
                Some((other, other_span)) => diagnostics.push(
                    Diagnostic::error(
                        codes::DUPLICATE_CODEPOINT,
                        format!("U+{cp:04X} is mapped by both `{other}` and `{name}`"),
                        Label::new(span.clone(), format!("`{name}` maps it here")),
                    )
                    .with_secondary(Label::new(
                        other_span.clone(),
                        format!("`{other}` maps it here"),
                    ))
                    .with_help("a codepoint maps to one glyph; remove it from one of them"),
                ),
                None => {
                    first_use.insert(cp, (name.as_str(), span.clone()));
                }
            }
        }
    }
    diagnostics
}
