//! Outline preparation for a whole instance (spec §3, §10.1–§10.4): every
//! glyph's [`mg_eval::glyph_outline`] through [`crate::outline`]'s stages,
//! plus the decisions that need the whole font — whether each component
//! stays a `glyf` composite or is decomposed (spec §10.1), how deep
//! components nest, and which glyph and path a filled contour that
//! quantization made self-intersecting came from (spec §10.4 step 4).

use indexmap::IndexMap;
use kurbo::Affine;
use mg_diag::{Diagnostic, Label, codes};
use mg_eval::{EvalOutcome, GlyphOutline, NodeId, OutlineContour, PlacedComponent};
use mg_geom::tolerance::{COMPONENT_DEPTH, Tolerances};
use mg_hir::model::{Hir, InstanceDecl};

use crate::outline::{self, OutlinePoint, PrepContext};

/// A glyph ready for `glyf` assembly: integer quadratic contours, or
/// components with their slant-conjugated transforms and rounded offsets
/// — never both, since a `glyf` glyph is either simple or composite.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreparedGlyph {
    pub contours: Vec<Vec<OutlinePoint>>,
    pub components: Vec<PreparedComponent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedComponent {
    pub glyph: String,
    /// `S·M·S⁻¹` (spec §10.1), its translation rounded per spec §10.4 and
    /// its 2×2 part within F2Dot14's range.
    pub transform: Affine,
}

/// A filled contour of `glyph`'s path at `path_index` crosses itself
/// after quantization (spec §10.4 step 4). `glyph` is where the path is
/// declared, which differs from the glyph being built when the contour
/// arrived through a decomposed component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantizedCrossing {
    pub glyph: String,
    pub path_index: usize,
}

/// The spatial accuracy of the post-quantization crossing test, in
/// design units. Coordinates are integers by then, so this only has to
/// resolve a crossing, not approximate one.
const INTERSECTION_ACCURACY: f64 = 1e-6;

/// The instance's slant θ in radians, `0` when upright (spec §12.3).
pub fn instance_slant(hir: &Hir, instance: &InstanceDecl) -> f64 {
    let font_em = hir.font.em.map(|em| em as f64);
    instance.slant.as_ref().map_or(0.0, |e| {
        mg_hir::const_eval::eval_const(e, font_em).expect("mg-hir guarantees `slant` is constant")
    })
}

/// The [`PrepContext`] for one instance: its shear, every metric's `.y`
/// and `.ink` as snap zones (spec §10.4), and the em-scaled tolerances.
pub fn prep_context(hir: &Hir, instance: &InstanceDecl, outcome: &EvalOutcome) -> PrepContext {
    let mut zones = Vec::new();
    for name in hir.metrics.keys() {
        if let Some(zone) = outcome
            .values
            .get(&NodeId::TopLevel(name.clone()))
            .and_then(|v| v.as_zone())
        {
            zones.push(zone.y);
            zones.push(zone.ink);
        }
    }
    PrepContext {
        shear: outline::shear(instance_slant(hir, instance)),
        zones,
        tolerances: Tolerances::for_em(hir.font.em.map_or(0.0, |em| em as f64)),
    }
}

/// Does `transform`'s 2×2 part fit `glyf`'s F2Dot14, each entry in
/// `[−2, 2)` (spec §10.1)?
pub fn fits_f2dot14(transform: Affine) -> bool {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    [a, b, c, d].iter().all(|v| (-2.0..2.0).contains(v))
}

/// Contours (each tagged with the glyph that declares its path) through
/// every preparation stage, re-checking filled ones for self-intersection
/// after quantization.
fn prepare_contours<'a>(
    contours: impl IntoIterator<Item = (&'a str, &'a OutlineContour)>,
    ctx: &PrepContext,
) -> (Vec<Vec<OutlinePoint>>, Vec<QuantizedCrossing>) {
    let mut prepared = Vec::new();
    let mut crossings = Vec::new();

    for (glyph, contour) in contours {
        for points in outline::prepare_contour(&contour.path, ctx) {
            if contour.filled {
                let pieces: Vec<kurbo::PathSeg> = outline::to_bezpath(&points).segments().collect();
                let hits =
                    mg_geom::fill::contour_self_intersections(&pieces, INTERSECTION_ACCURACY);
                let reported = QuantizedCrossing {
                    glyph: glyph.to_string(),
                    path_index: contour.path_index,
                };
                if !hits.is_empty() && !crossings.contains(&reported) {
                    crossings.push(reported);
                }
            }
            prepared.push(points);
        }
    }

    (prepared, crossings)
}

/// A component as `glyf` stores it: `S·M·S⁻¹` with a rounded offset.
fn prepare_component(component: &PlacedComponent, ctx: &PrepContext) -> PreparedComponent {
    let [a, b, c, d, e, f] = outline::conjugate(ctx.shear, component.transform).as_coeffs();
    PreparedComponent {
        glyph: component.glyph.clone(),
        transform: Affine::new([a, b, c, d, e.round(), f.round()]),
    }
}

/// One glyph's outline, prepared without reference to any other glyph:
/// its own contours, and its components kept as components. Used where
/// decomposition is not in play; [`prepare_font`] decides that per glyph.
pub fn prepare_glyph(
    name: &str,
    glyph: &GlyphOutline,
    ctx: &PrepContext,
) -> (PreparedGlyph, Vec<QuantizedCrossing>) {
    let (contours, crossings) = prepare_contours(glyph.contours.iter().map(|c| (name, c)), ctx);
    let components = glyph
        .components
        .iter()
        .map(|c| prepare_component(c, ctx))
        .collect();
    (
        PreparedGlyph {
            contours,
            components,
        },
        crossings,
    )
}

/// `name`'s contours with every component's own placed in, all still
/// unrounded and upright, each tagged with the glyph declaring its path.
/// A placement with a reflection reverses its contours' direction, so
/// each is re-oriented for its role (spec §8.2).
fn flatten(
    outlines: &IndexMap<String, GlyphOutline>,
    name: &str,
    transform: Affine,
    depth: usize,
    out: &mut Vec<(String, OutlineContour)>,
) {
    let Some(glyph) = outlines.get(name) else {
        return;
    };
    for contour in &glyph.contours {
        let mut placed = contour.clone();
        placed.path.apply_affine(transform);
        mg_geom::winding::orient_for_glyf(&mut placed.path, placed.role);
        out.push((name.to_string(), placed));
    }
    if depth >= COMPONENT_DEPTH {
        return;
    }
    for component in &glyph.components {
        flatten(
            outlines,
            &component.glyph,
            transform * component.transform,
            depth + 1,
            out,
        );
    }
}

/// How many component levels hang below `name`: `0` for a glyph without
/// components, `1` for one whose components are all simple, and so on.
/// Capped just past [`COMPONENT_DEPTH`], which is all the limit check
/// needs and keeps a cycle (already an evaluation error) from recursing
/// forever in a preview.
fn component_depth(outlines: &IndexMap<String, GlyphOutline>, name: &str, seen: usize) -> usize {
    let Some(glyph) = outlines.get(name) else {
        return 0;
    };
    if glyph.components.is_empty() || seen > COMPONENT_DEPTH {
        return 0;
    }
    1 + glyph
        .components
        .iter()
        .map(|c| component_depth(outlines, &c.glyph, seen + 1))
        .max()
        .unwrap_or(0)
}

/// Every default-set glyph of `instance`, in declaration order (spec §14),
/// prepared for `glyf`. `outcome` is that instance's evaluation; callers
/// building a font stop before this when it has any error (spec §4.6).
///
/// A glyph stays composite when it has no contours of its own and every
/// component's `S·M·S⁻¹` fits F2Dot14 (spec §10.1). Otherwise every
/// component is decomposed — a `glyf` glyph cannot mix contours and
/// components — by placing the component glyphs' unrounded outlines and
/// preparing the result as one glyph, since extrema, cu2qu, and rounding
/// all depend on the final placement.
pub fn prepare_font(
    hir: &Hir,
    instance: &InstanceDecl,
    outcome: &EvalOutcome,
) -> (IndexMap<String, PreparedGlyph>, Vec<Diagnostic>) {
    let ctx = prep_context(hir, instance, outcome);
    let mut diagnostics = Vec::new();

    let mut outlines: IndexMap<String, GlyphOutline> = IndexMap::new();
    for (name, glyphset) in hir.glyphs.keys() {
        if glyphset.is_none() {
            let outline = mg_eval::glyph_outline(
                hir,
                instance,
                name,
                &outcome.values,
                &outcome.failed,
                &mut diagnostics,
            );
            outlines.insert(name.clone(), outline);
        }
    }

    let mut glyphs = IndexMap::new();
    for (name, glyph) in &outlines {
        let depth = component_depth(&outlines, name, 0);
        if depth > COMPONENT_DEPTH {
            let decl = effective_glyph(hir, instance, name);
            diagnostics.push(
                Diagnostic::error(
                    codes::COMPONENT_TOO_DEEP,
                    format!(
                        "components nest more than {COMPONENT_DEPTH} levels deep under glyph `{name}`"
                    ),
                    Label::new(mg_syntax::trimmed_range(&decl.syntax), "in this glyph"),
                )
                .with_help("decompose one level by drawing its paths directly"),
            );
            glyphs.insert(name.clone(), PreparedGlyph::default());
            continue;
        }

        let composite = glyph.contours.is_empty()
            && !glyph.components.is_empty()
            && glyph
                .components
                .iter()
                .all(|c| fits_f2dot14(outline::conjugate(ctx.shear, c.transform)));

        let (prepared, crossings) = if composite {
            prepare_glyph(name, glyph, &ctx)
        } else {
            let mut flat = Vec::new();
            flatten(&outlines, name, Affine::IDENTITY, 0, &mut flat);
            let (contours, crossings) =
                prepare_contours(flat.iter().map(|(g, c)| (g.as_str(), c)), &ctx);
            (
                PreparedGlyph {
                    contours,
                    components: Vec::new(),
                },
                crossings,
            )
        };

        for crossing in crossings {
            let decl = effective_glyph(hir, instance, &crossing.glyph);
            let path = &decl.paths[crossing.path_index];
            let mut diagnostic = quantized_crossing_diagnostic(
                &crossing.glyph,
                path.name.as_deref(),
                mg_syntax::trimmed_range(&path.syntax),
            );
            if crossing.glyph != *name {
                diagnostic =
                    diagnostic.with_note(format!("placed as a component of glyph `{name}`"));
            }
            diagnostics.push(diagnostic);
        }
        glyphs.insert(name.clone(), prepared);
    }

    (glyphs, diagnostics)
}

fn effective_glyph<'a>(
    hir: &'a Hir,
    instance: &InstanceDecl,
    name: &str,
) -> &'a mg_hir::model::GlyphDecl {
    mg_eval::graph::effective_glyph(hir, instance, name)
        .expect("mg-hir guarantees every alternate has a default")
}

fn quantized_crossing_diagnostic(
    glyph: &str,
    path: Option<&str>,
    span: std::ops::Range<usize>,
) -> Diagnostic {
    let what = match path {
        Some(path) => format!("filled path `{path}` in glyph `{glyph}`"),
        None => format!("a filled path in glyph `{glyph}`"),
    };
    Diagnostic::error(
        codes::QUANTIZED_FILL_SELF_INTERSECTS,
        format!("{what} crosses itself after rounding to integer coordinates"),
        Label::new(
            span,
            "this contour is simple before rounding, but not after",
        ),
    )
    .with_help("two parts of the contour pass within a unit of each other; move them further apart")
}
