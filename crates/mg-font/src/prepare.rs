//! Outline preparation for a whole instance (spec §3, §10.1–§10.4): every
//! glyph's [`mg_eval::glyph_outline`] through [`crate::outline`]'s stages,
//! plus the one check that needs the HIR to report — a filled contour
//! that quantization made self-intersecting (spec §10.4 step 4).

use indexmap::IndexMap;
use kurbo::Affine;
use mg_diag::{Diagnostic, Label, codes};
use mg_eval::{EvalOutcome, GlyphOutline, NodeId};
use mg_geom::tolerance::Tolerances;
use mg_hir::model::{Hir, InstanceDecl};

use crate::outline::{self, OutlinePoint, PrepContext};

/// A glyph ready for `glyf` assembly: integer quadratic contours, and
/// components with their slant-conjugated transforms and rounded offsets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreparedGlyph {
    pub contours: Vec<Vec<OutlinePoint>>,
    pub components: Vec<PreparedComponent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedComponent {
    pub glyph: String,
    /// `S·M·S⁻¹` (spec §10.1), its translation rounded per spec §10.4.
    /// Whether the 2×2 part fits F2Dot14, and so whether this stays a
    /// composite, is table assembly's decision.
    pub transform: Affine,
}

/// A filled contour of the path at `path_index` crosses itself after
/// quantization (spec §10.4 step 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantizedCrossing {
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

/// One glyph's outline through every preparation stage. A filled contour
/// that quantization made self-intersecting is still returned in the
/// glyph, and reported alongside it.
pub fn prepare_glyph(
    glyph: &GlyphOutline,
    ctx: &PrepContext,
) -> (PreparedGlyph, Vec<QuantizedCrossing>) {
    let mut prepared = PreparedGlyph::default();
    let mut crossings = Vec::new();

    for contour in &glyph.contours {
        for points in outline::prepare_contour(&contour.path, ctx) {
            if contour.filled {
                let pieces: Vec<kurbo::PathSeg> = outline::to_bezpath(&points).segments().collect();
                let hits =
                    mg_geom::fill::contour_self_intersections(&pieces, INTERSECTION_ACCURACY);
                let reported = QuantizedCrossing {
                    path_index: contour.path_index,
                };
                if !hits.is_empty() && !crossings.contains(&reported) {
                    crossings.push(reported);
                }
            }
            prepared.contours.push(points);
        }
    }

    for component in &glyph.components {
        let mut transform = outline::conjugate(ctx.shear, component.transform);
        let [a, b, c, d, e, f] = transform.as_coeffs();
        transform = Affine::new([a, b, c, d, e.round(), f.round()]);
        prepared.components.push(PreparedComponent {
            glyph: component.glyph.clone(),
            transform,
        });
    }

    (prepared, crossings)
}

/// Every default-set glyph of `instance`, in declaration order (spec §14),
/// prepared for `glyf`. `outcome` is that instance's evaluation; callers
/// building a font stop before this when it has any error (spec §4.6).
pub fn prepare_font(
    hir: &Hir,
    instance: &InstanceDecl,
    outcome: &EvalOutcome,
) -> (IndexMap<String, PreparedGlyph>, Vec<Diagnostic>) {
    let ctx = prep_context(hir, instance, outcome);
    let mut glyphs = IndexMap::new();
    let mut diagnostics = Vec::new();

    for (name, glyphset) in hir.glyphs.keys() {
        if glyphset.is_some() {
            continue;
        }
        let glyph_outline = mg_eval::glyph_outline(
            hir,
            instance,
            name,
            &outcome.values,
            &outcome.failed,
            &mut diagnostics,
        );
        let (prepared, crossings) = prepare_glyph(&glyph_outline, &ctx);
        for crossing in crossings {
            let decl = mg_eval::graph::effective_glyph(hir, instance, name)
                .expect("mg-hir guarantees every alternate has a default");
            let path = &decl.paths[crossing.path_index];
            diagnostics.push(quantized_crossing_diagnostic(
                name,
                path.name.as_deref(),
                &instance.name,
                mg_syntax::trimmed_range(&path.syntax),
            ));
        }
        glyphs.insert(name.clone(), prepared);
    }

    (glyphs, diagnostics)
}

fn quantized_crossing_diagnostic(
    glyph: &str,
    path: Option<&str>,
    instance: &str,
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
    .with_note(format!("in instance `{instance}`"))
    .with_help("two parts of the contour pass within a unit of each other; move them further apart")
}
