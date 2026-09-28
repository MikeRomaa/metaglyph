//! TTF table assembly (spec §10.1, §10.5–§10.6, §11.4, §12.1, §12.3):
//! prepared glyphs and per-instance font data in, a complete `.ttf` out.
//! Pure — no HIR, no evaluation. The limit checks that can fail (spec
//! §10.5) come back as [`LimitError`]s keyed by glyph index, and
//! [`crate::build`] turns them into diagnostics pointing at the source.

use kurbo::Affine;
use read_fonts::tables::glyf::CurvePoint;
use write_fonts::FontBuilder;
use write_fonts::OffsetMarker;
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::gasp::{Gasp, GaspRange, GaspRangeBehavior};
use write_fonts::tables::glyf::{
    Anchor, Bbox, Component, ComponentFlags, CompositeGlyph, Contour, GlyfLocaBuilder, Glyph,
    SimpleGlyph, Transform,
};
use write_fonts::tables::head::{Flags, Head, MacStyle};
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::{Hmtx, LongMetric};
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::{Os2, SelectionFlags};
use write_fonts::tables::post::Post;
use write_fonts::types::{
    F2Dot14, FWord, Fixed, GlyphId, GlyphId16, LongDateTime, NameId, Tag, UfWord,
};

use crate::kern::{self, KernRule};
use crate::prepare::PreparedGlyph;

/// One glyph in final glyph order (`.notdef` first, spec §10.6).
#[derive(Debug, Clone)]
pub struct GlyphRecord {
    pub name: String,
    pub codepoints: Vec<u32>,
    /// Rounded per spec §10.4; range-checked here.
    pub advance: i64,
    pub glyph: PreparedGlyph,
}

/// Everything besides the glyphs that one instance's tables need.
#[derive(Debug, Clone)]
pub struct FontInfo {
    pub em: u16,
    pub family: String,
    pub style: String,
    /// `font.version`, e.g. `"1.000"`.
    pub version: String,
    pub designer: Option<String>,
    pub foundry: Option<String>,
    pub license: Option<String>,
    pub weight_class: u16,
    pub width_class: u16,
    /// The instance slant θ, radians.
    pub slant: f64,
    /// `ascender.y`, `descender.y`, `capHeight.y`, `xHeight.y`, unrounded.
    pub ascender: f64,
    pub descender: f64,
    pub cap_height: f64,
    pub x_height: f64,
    /// `head.created`/`head.modified`, seconds since the Unix epoch.
    pub timestamp: i64,
}

/// A spec §10.5 limit that glyph `glyph` (an index into the records)
/// breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitError {
    CoordinateOutOfRange {
        glyph: usize,
    },
    TooManyPoints {
        glyph: usize,
        points: usize,
    },
    TooManyContours {
        glyph: usize,
        contours: usize,
    },
    AdvanceOutOfRange {
        glyph: usize,
        advance: i64,
    },
    TooManyGlyphs {
        count: usize,
    },
    /// `kern` number `kern` (an index into the rules) rounds to a value
    /// outside int16.
    KernOutOfRange {
        kern: usize,
        value: i64,
    },
}

/// Seconds from 1904-01-01 (`LongDateTime`'s epoch) to 1970-01-01.
const MAC_EPOCH_OFFSET: i64 = 2_082_844_800;

/// A component's transform as `glyf` stores it: the 2×2 part in F2Dot14,
/// the offset in int16. The caller has already checked both fit.
fn quantized_transform(t: Affine) -> (Transform, i16, i16) {
    // kurbo maps (x, y) to (a·x + c·y + e, b·x + d·y + f). `glyf` stores
    // xscale, scale01, scale10, yscale, and maps x' = xscale·x +
    // scale10·y, y' = scale01·x + yscale·y — so scale01 is b, and
    // scale10 is c. read-fonts names them `yx` and `xy`, in that order.
    let [a, b, c, d, e, f] = t.as_coeffs();
    (
        Transform {
            xx: F2Dot14::from_f64(a),
            yx: F2Dot14::from_f64(b),
            xy: F2Dot14::from_f64(c),
            yy: F2Dot14::from_f64(d),
        },
        e as i16,
        f as i16,
    )
}

/// The affine a rasterizer applies for a component: the F2Dot14-rounded
/// 2×2 part and the integer offset.
fn effective_affine(t: Affine) -> Affine {
    let (m, dx, dy) = quantized_transform(t);
    Affine::new([
        m.xx.to_f64(),
        m.yx.to_f64(),
        m.xy.to_f64(),
        m.yy.to_f64(),
        dx as f64,
        dy as f64,
    ])
}

/// Every point of glyph `index` with its components decomposed, as a
/// rasterizer would place them, plus its total contour count and its
/// component nesting depth. Rounded half away from zero.
fn decomposed(
    glyphs: &[GlyphRecord],
    ids: &indexmap::IndexMap<&str, usize>,
    index: usize,
    transform: Affine,
    depth: usize,
    out: &mut Vec<(i64, i64)>,
) -> (usize, usize) {
    let glyph = &glyphs[index].glyph;
    let mut contours = glyph.contours.len();
    for contour in &glyph.contours {
        for p in contour {
            let q = transform * kurbo::Point::new(p.x as f64, p.y as f64);
            out.push((q.x.round() as i64, q.y.round() as i64));
        }
    }
    let mut max_depth = 0;
    // `mg-font`'s own depth check caps nesting well before this could
    // recurse without bound.
    if depth <= mg_geom::tolerance::COMPONENT_DEPTH {
        for component in &glyph.components {
            let Some(&target) = ids.get(component.glyph.as_str()) else {
                continue;
            };
            let (c, d) = decomposed(
                glyphs,
                ids,
                target,
                transform * effective_affine(component.transform),
                depth + 1,
                out,
            );
            contours += c;
            max_depth = max_depth.max(d + 1);
        }
    }
    (contours, max_depth)
}

fn bbox_of(points: &[(i64, i64)]) -> Option<(i64, i64, i64, i64)> {
    let (&(x0, y0), rest) = points.split_first()?;
    Some(rest.iter().fold((x0, y0, x0, y0), |(a, b, c, d), &(x, y)| {
        (a.min(x), b.min(y), c.max(x), d.max(y))
    }))
}

fn in_i16(v: i64) -> bool {
    (i16::MIN as i64..=i16::MAX as i64).contains(&v)
}

fn round(v: f64) -> i16 {
    v.round() as i16
}

/// Per-glyph facts every table below draws on.
struct Measured {
    /// `(xMin, yMin, xMax, yMax)` of the decomposed outline; `None` when
    /// it has no points.
    bbox: Option<(i64, i64, i64, i64)>,
    points: usize,
    contours: usize,
    depth: usize,
}

/// RIBBI style names (spec §12.3).
fn is_ribbi(style: &str) -> bool {
    matches!(style, "Regular" | "Italic" | "Bold" | "Bold Italic")
}

/// Builds the whole font, or reports every limit it breaks.
pub fn assemble(
    info: &FontInfo,
    glyphs: &[GlyphRecord],
    kerns: &[KernRule],
) -> Result<Vec<u8>, Vec<LimitError>> {
    let mut errors = Vec::new();
    if glyphs.len() > u16::MAX as usize {
        return Err(vec![LimitError::TooManyGlyphs {
            count: glyphs.len(),
        }]);
    }

    let ids: indexmap::IndexMap<&str, usize> = glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| (g.name.as_str(), i))
        .collect();

    // -- measure, and check spec §10.5's limits ---------------------------

    let mut measured = Vec::with_capacity(glyphs.len());
    for (i, record) in glyphs.iter().enumerate() {
        let mut points = Vec::new();
        let (contours, depth) = decomposed(glyphs, &ids, i, Affine::IDENTITY, 0, &mut points);
        let bbox = bbox_of(&points);

        let own_coords_fit = record
            .glyph
            .contours
            .iter()
            .flatten()
            .all(|p| in_i16(p.x as i64) && in_i16(p.y as i64));
        let offsets_fit = record.glyph.components.iter().all(|c| {
            let [_, _, _, _, e, f] = c.transform.as_coeffs();
            in_i16(e as i64) && in_i16(f as i64)
        });
        let bbox_fits = bbox.is_none_or(|(a, b, c, d)| [a, b, c, d].into_iter().all(in_i16));
        if !(own_coords_fit && offsets_fit && bbox_fits) {
            errors.push(LimitError::CoordinateOutOfRange { glyph: i });
        }
        if points.len() > u16::MAX as usize {
            errors.push(LimitError::TooManyPoints {
                glyph: i,
                points: points.len(),
            });
        }
        if contours > u16::MAX as usize {
            errors.push(LimitError::TooManyContours { glyph: i, contours });
        }
        if !(0..=u16::MAX as i64).contains(&record.advance) {
            errors.push(LimitError::AdvanceOutOfRange {
                glyph: i,
                advance: record.advance,
            });
        }
        measured.push(Measured {
            bbox,
            points: points.len(),
            contours,
            depth,
        });
    }
    for (kern, rule) in kerns.iter().enumerate() {
        if !in_i16(rule.value) {
            errors.push(LimitError::KernOutOfRange {
                kern,
                value: rule.value,
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // -- glyf / loca ------------------------------------------------------

    let mut glyf_builder = GlyfLocaBuilder::new();
    for (record, m) in glyphs.iter().zip(&measured) {
        let bbox = m.bbox.map_or(Bbox::default(), |(a, b, c, d)| Bbox {
            x_min: a as i16,
            y_min: b as i16,
            x_max: c as i16,
            y_max: d as i16,
        });
        let glyph = if !record.glyph.components.is_empty() {
            let mut composite: Option<CompositeGlyph> = None;
            for (k, component) in record.glyph.components.iter().enumerate() {
                let (transform, dx, dy) = quantized_transform(component.transform);
                let gid = ids[component.glyph.as_str()];
                let flags = ComponentFlags {
                    round_xy_to_grid: true,
                    // spec §8: overlaps are kept; the flag goes on the
                    // first component.
                    overlap_compound: k == 0,
                    ..Default::default()
                };
                let component = Component::new(
                    GlyphId16::new(gid as u16),
                    Anchor::Offset { x: dx, y: dy },
                    transform,
                    flags,
                );
                match &mut composite {
                    None => composite = Some(CompositeGlyph::new(component, bbox)),
                    Some(c) => c.add_component(component, bbox),
                }
            }
            Glyph::Composite(composite.expect("at least one component"))
        } else if record.glyph.contours.is_empty() {
            Glyph::Empty
        } else {
            let contours = record
                .glyph
                .contours
                .iter()
                .map(|c| {
                    Contour::from(
                        c.iter()
                            .map(|p| CurvePoint {
                                x: p.x as i16,
                                y: p.y as i16,
                                on_curve: p.on_curve,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            Glyph::Simple(SimpleGlyph {
                bbox,
                contours,
                instructions: Vec::new(),
                overlaps: true,
            })
        };
        glyf_builder
            .add_glyph(&glyph)
            .expect("every glyph was range-checked above");
    }
    let (glyf, loca, loca_format) = glyf_builder.build();

    // -- metrics ------------------------------------------------------------

    let lsb = |m: &Measured| m.bbox.map_or(0, |b| b.0) as i16;
    let hmtx = Hmtx::new(
        glyphs
            .iter()
            .zip(&measured)
            .map(|(g, m)| LongMetric::new(g.advance as u16, lsb(m)))
            .collect(),
        Vec::new(),
    );

    let inked: Vec<(i64, (i64, i64, i64, i64))> = glyphs
        .iter()
        .zip(&measured)
        .filter_map(|(g, m)| m.bbox.map(|b| (g.advance, b)))
        .collect();
    let font_bbox = inked
        .iter()
        .map(|&(_, b)| b)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)));
    let (x_min, y_min, x_max, y_max) = font_bbox.unwrap_or((0, 0, 0, 0));
    let min_lsb = inked.iter().map(|&(_, b)| b.0).min().unwrap_or(0);
    let min_rsb = inked.iter().map(|&(adv, b)| adv - b.2).min().unwrap_or(0);
    let x_max_extent = inked.iter().map(|&(_, b)| b.2).max().unwrap_or(0);
    let advance_max = glyphs.iter().map(|g| g.advance).max().unwrap_or(0);

    let em = info.em as f64;
    let tan = info.slant.tan();

    let hhea = Hhea::new(
        FWord::new(round(info.ascender)),
        FWord::new(round(info.descender)),
        FWord::new(0),
        UfWord::new(advance_max as u16),
        FWord::new(min_lsb as i16),
        FWord::new(min_rsb as i16),
        FWord::new(x_max_extent as i16),
        info.em as i16,
        round(em * tan),
        0,
        glyphs.len() as u16,
    );

    let composites: Vec<&Measured> = glyphs
        .iter()
        .zip(&measured)
        .filter(|(g, _)| !g.glyph.components.is_empty())
        .map(|(_, m)| m)
        .collect();
    let simples = || {
        glyphs
            .iter()
            .filter(|g| g.glyph.components.is_empty())
            .map(|g| &g.glyph)
    };
    let maxp = Maxp {
        num_glyphs: glyphs.len() as u16,
        max_points: Some(
            simples()
                .map(|g| g.contours.iter().map(Vec::len).sum::<usize>())
                .max()
                .unwrap_or(0) as u16,
        ),
        max_contours: Some(simples().map(|g| g.contours.len()).max().unwrap_or(0) as u16),
        max_composite_points: Some(composites.iter().map(|m| m.points).max().unwrap_or(0) as u16),
        max_composite_contours: Some(
            composites.iter().map(|m| m.contours).max().unwrap_or(0) as u16
        ),
        max_zones: Some(1),
        max_twilight_points: Some(0),
        max_storage: Some(0),
        max_function_defs: Some(0),
        max_instruction_defs: Some(0),
        max_stack_elements: Some(0),
        max_size_of_instructions: Some(0),
        max_component_elements: Some(
            glyphs
                .iter()
                .map(|g| g.glyph.components.len())
                .max()
                .unwrap_or(0) as u16,
        ),
        max_component_depth: Some(composites.iter().map(|m| m.depth).max().unwrap_or(0) as u16),
    };

    // -- naming and style bits (spec §12.3) -------------------------------

    let ribbi = is_ribbi(&info.style);
    let bold = ribbi && info.style.starts_with("Bold");
    let italic = ribbi && info.style.ends_with("Italic");
    let mut selection = SelectionFlags::USE_TYPO_METRICS;
    let mut mac_style = MacStyle::empty();
    if bold {
        selection |= SelectionFlags::BOLD;
        mac_style |= MacStyle::BOLD;
    }
    if italic {
        selection |= SelectionFlags::ITALIC;
        mac_style |= MacStyle::ITALIC;
    }
    if !bold && !italic {
        selection |= SelectionFlags::REGULAR;
    }

    let codepoints: Vec<u32> = glyphs
        .iter()
        .flat_map(|g| g.codepoints.iter().copied())
        .collect();
    let advances: Vec<i64> = glyphs
        .iter()
        .map(|g| g.advance)
        .filter(|&a| a > 0)
        .collect();
    let avg_width = if advances.is_empty() {
        0
    } else {
        (advances.iter().sum::<i64>() as f64 / advances.len() as f64).round() as i16
    };
    let has_basic_latin = codepoints.iter().any(|&c| c < 0x80);

    let os2 = Os2 {
        x_avg_char_width: avg_width,
        us_weight_class: info.weight_class,
        us_width_class: info.width_class,
        fs_type: 0,
        // ufo2ft's defaults, as fractions of the em.
        y_subscript_x_size: round(em * 0.65),
        y_subscript_y_size: round(em * 0.6),
        y_subscript_x_offset: round(-em * 0.075 * tan),
        y_subscript_y_offset: round(em * 0.075),
        y_superscript_x_size: round(em * 0.65),
        y_superscript_y_size: round(em * 0.6),
        y_superscript_x_offset: round(em * 0.35 * tan),
        y_superscript_y_offset: round(em * 0.35),
        y_strikeout_size: round(em / 20.0),
        y_strikeout_position: round(info.x_height / 2.0),
        s_family_class: 0,
        panose_10: [0; 10],
        ul_unicode_range_1: unicode_range_1(&codepoints),
        ul_unicode_range_2: 0,
        ul_unicode_range_3: 0,
        ul_unicode_range_4: 0,
        ach_vend_id: Tag::new(b"NONE"),
        fs_selection: selection,
        us_first_char_index: codepoints.iter().min().map_or(0, |&c| c.min(0xFFFF)) as u16,
        us_last_char_index: codepoints.iter().max().map_or(0, |&c| c.min(0xFFFF)) as u16,
        s_typo_ascender: round(info.ascender),
        s_typo_descender: round(info.descender),
        s_typo_line_gap: 0,
        us_win_ascent: y_max.max(0) as u16,
        us_win_descent: (-y_min).max(0) as u16,
        // Code page 1252 (Latin 1).
        ul_code_page_range_1: Some(u32::from(has_basic_latin)),
        ul_code_page_range_2: Some(0),
        sx_height: Some(round(info.x_height)),
        s_cap_height: Some(round(info.cap_height)),
        us_default_char: Some(0),
        us_break_char: Some(0x20),
        // A pair kern looks at two glyphs.
        us_max_context: Some(if kerns.is_empty() { 0 } else { 2 }),
        us_lower_optical_point_size: None,
        us_upper_optical_point_size: None,
    };

    let ps_name = format!(
        "{}-{}",
        info.family.replace(' ', ""),
        info.style.replace(' ', "")
    );
    let (name_1, name_2) = if ribbi {
        (info.family.clone(), info.style.clone())
    } else {
        (
            format!("{} {}", info.family, info.style),
            "Regular".to_string(),
        )
    };
    let mut names: Vec<(u16, String)> = vec![
        (1, name_1),
        (2, name_2),
        (3, format!("{};{ps_name}", info.version)),
        (4, format!("{} {}", info.family, info.style)),
        (5, format!("Version {}", info.version)),
        (6, ps_name),
    ];
    if let Some(foundry) = &info.foundry {
        names.push((8, foundry.clone()));
    }
    if let Some(designer) = &info.designer {
        names.push((9, designer.clone()));
    }
    if let Some(license) = &info.license {
        names.push((13, license.clone()));
    }
    if !ribbi {
        names.push((16, info.family.clone()));
        names.push((17, info.style.clone()));
    }
    names.sort_by_key(|(id, _)| *id);
    let name = Name::new(
        names
            .into_iter()
            .map(|(id, s)| NameRecord::new(3, 1, 0x409, NameId::new(id), OffsetMarker::new(s)))
            .collect(),
    );

    let glyph_names: Vec<&str> = glyphs.iter().map(|g| g.name.as_str()).collect();
    let mut post = Post::new_v2(glyph_names);
    post.italic_angle = Fixed::from_f64(-info.slant.to_degrees());
    post.underline_position = FWord::new(round(-em / 10.0));
    post.underline_thickness = FWord::new(round(em / 20.0));

    let timestamp = LongDateTime::new(info.timestamp + MAC_EPOCH_OFFSET);
    let head = Head::new(
        Fixed::from_f64(info.version.parse::<f64>().unwrap_or(1.0)),
        0,
        // Baseline at y = 0, left sidebearing at x = xMin, integer ppem.
        Flags::from_bits_truncate(0x000B),
        info.em,
        timestamp,
        timestamp,
        x_min as i16,
        y_min as i16,
        x_max as i16,
        y_max as i16,
        mac_style,
        6,
        loca_format as i16,
    );

    let mappings = glyphs.iter().enumerate().flat_map(|(gid, g)| {
        g.codepoints
            .iter()
            .filter_map(move |&cp| char::from_u32(cp).map(|ch| (ch, GlyphId::new(gid as u32))))
    });
    let cmap = Cmap::from_mappings(mappings).expect("duplicate codepoints were rejected earlier");

    // spec §11.4: one range, gridfit + grayscale + symmetric both ways.
    let gasp = Gasp::new(
        1,
        1,
        vec![GaspRange::new(
            0xFFFF,
            GaspRangeBehavior::from_bits_truncate(0x000F),
        )],
    );

    let mut builder = FontBuilder::new();
    builder
        .add_table(&head)
        .and_then(|b| b.add_table(&hhea))
        .and_then(|b| b.add_table(&maxp))
        .and_then(|b| b.add_table(&os2))
        .and_then(|b| b.add_table(&hmtx))
        .and_then(|b| b.add_table(&cmap))
        .and_then(|b| b.add_table(&loca))
        .and_then(|b| b.add_table(&glyf))
        .and_then(|b| b.add_table(&name))
        .and_then(|b| b.add_table(&post))
        .and_then(|b| b.add_table(&gasp))
        .expect("every table is well-formed by construction");
    if let Some(gpos) = kern::build_gpos(kerns, &ids) {
        builder
            .add_table(&gpos)
            .expect("GPOS is well-formed by construction");
    }
    Ok(builder.build())
}

/// `OS/2.ulUnicodeRange1` bits for the Latin blocks (bits 0–3). Other
/// blocks are left clear; nothing in spec §12 asks for them.
fn unicode_range_1(codepoints: &[u32]) -> u32 {
    const BLOCKS: [(u32, u32, u32); 4] = [
        (0, 0x0000, 0x007F),
        (1, 0x0080, 0x00FF),
        (2, 0x0100, 0x017F),
        (3, 0x0180, 0x024F),
    ];
    BLOCKS
        .iter()
        .filter(|(_, lo, hi)| codepoints.iter().any(|c| (lo..=hi).contains(&c)))
        .fold(0, |bits, (bit, _, _)| bits | (1 << bit))
}
