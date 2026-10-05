//! Whole-glyph rendering (spec §6.5, §8.1, §10.1): every rendering path's
//! contours, with the glyph's own filled contours' nesting resolved
//! across paths, plus every component — as a placed reference
//! ([`glyph_outline`], what `mg-font` compiles) or decomposed into its
//! own sub-render ([`render_glyph`], what `mg svg` previews).
//! `crate::eval::render_path` is the per-path primitive this builds on.

use indexmap::{IndexMap, IndexSet};
use kurbo::{Affine, BezPath};
use mg_diag::Diagnostic;
use mg_hir::model::{Hir, InstanceDecl};

use crate::eval::{self, EvalCtx, component_affine};
use crate::graph::{self, NodeId};
use crate::value::Value;
use mg_geom::tolerance::COMPONENT_DEPTH;
use mg_geom::winding::ContourRole;

/// One contour of a glyph's own outline, oriented for `glyf` (spec §8.2),
/// with where it came from: `mg-font` re-checks a filled contour for
/// self-intersection after quantization (spec §10.4) and names the path
/// when that fails.
#[derive(Debug, Clone)]
pub struct OutlineContour {
    pub path: BezPath,
    pub role: ContourRole,
    /// The declaration that drew it, in the effective glyph.
    pub source: ContourSource,
    /// `true` for a `fill`'s own contour, `false` for a stroke's.
    pub filled: bool,
}

/// What drew an [`OutlineContour`]: one of the glyph's paths, or one of
/// its path components (spec §5.7), by index into `paths` or `components`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContourSource {
    Path(usize),
    Component(usize),
}

/// A component reference with its placement evaluated (spec §10.1).
#[derive(Debug, Clone)]
pub struct PlacedComponent {
    pub glyph: String,
    pub transform: Affine,
}

/// A glyph's outline before any font-compiler stage (spec §3): its own
/// contours with roles and winding resolved, and its components still as
/// references rather than decomposed.
#[derive(Debug, Clone, Default)]
pub struct GlyphOutline {
    pub contours: Vec<OutlineContour>,
    pub components: Vec<PlacedComponent>,
}

/// `glyph_name`'s own outline (spec §6.5, §8): every rendering path's
/// contours, with the glyph's filled contours' nesting resolved across
/// paths (spec §8.1) and every contour oriented for `glyf` (spec §8.2),
/// plus each component's evaluated placement.
///
/// A path or component that failed during `evaluate()` — `failed` is that
/// same call's `EvalOutcome::failed` — is skipped rather than aborting
/// the rest. Its diagnostic was already produced by that `evaluate()`
/// call, so nothing is re-reported here. `mg build` refuses to go this
/// far with any failure (spec §4.6), so only `mg svg`'s preview ever sees
/// a partial outline.
pub fn glyph_outline(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    values: &IndexMap<NodeId, Value>,
    failed: &IndexSet<NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> GlyphOutline {
    let glyph = graph::effective_glyph(hir, instance, glyph_name)
        .expect("mg-hir guarantees every alternate has a default");

    let mut outline = GlyphOutline::default();
    let mut fill_slots: Vec<usize> = Vec::new();

    for (path_index, path) in glyph.paths.iter().enumerate() {
        if !path.renders() {
            continue;
        }
        // `PathBbox` covers exactly what a render needs (`PathRealized`
        // plus `stroke`'s own references, spec plan M4's dependency
        // wiring) without this crate having to re-derive that list here.
        if failed.contains(&NodeId::PathBbox(glyph_name.to_string(), path_index)) {
            continue;
        }
        let Ok(path_contours) =
            eval::render_path(hir, instance, glyph_name, path_index, values, diagnostics)
        else {
            continue;
        };
        for (i, (contour, role)) in path_contours.into_iter().enumerate() {
            // `render_path` always pushes a `fill`ed path's own contour
            // first, before any stroke contours.
            let filled = path.fill && i == 0;
            if filled {
                fill_slots.push(outline.contours.len());
            }
            outline.contours.push(OutlineContour {
                path: contour,
                role,
                source: ContourSource::Path(path_index),
                filled,
            });
        }
    }

    // Path components (spec §5.7, §10.1) draw this glyph's own contours,
    // so their fills count in the role nesting below like a path's.
    for (index, component) in glyph.components.iter().enumerate() {
        if component.path.is_none()
            || failed.contains(&NodeId::ComponentBbox(glyph_name.to_string(), index))
        {
            continue;
        }
        let Ok(component_contours) =
            eval::render_path_component(hir, instance, glyph_name, index, values, diagnostics)
        else {
            continue;
        };
        let source_fills = component.fill.unwrap_or_else(|| {
            component
                .source_path(glyph_name)
                .and_then(|(g, p)| {
                    graph::effective_glyph(hir, instance, &g)?
                        .path_named(&p)
                        .map(|path| path.fill)
                })
                .unwrap_or(false)
        });
        for (i, (contour, role)) in component_contours.into_iter().enumerate() {
            let filled = source_fills && i == 0;
            if filled {
                fill_slots.push(outline.contours.len());
            }
            outline.contours.push(OutlineContour {
                path: contour,
                role,
                source: ContourSource::Component(index),
                filled,
            });
        }
    }

    if fill_slots.len() > 1 {
        let fill_paths: Vec<BezPath> = fill_slots
            .iter()
            .map(|&i| outline.contours[i].path.clone())
            .collect();
        for (&slot, role) in fill_slots
            .iter()
            .zip(mg_geom::winding::resolve_fill_roles(&fill_paths))
        {
            outline.contours[slot].role = role;
        }
    }

    // Everything below is placed (spec §12.1): the glyph's own ink moves
    // by its shift, and so does every component, on top of its placement.
    // A failed shift leaves a preview in authored coordinates.
    let shift = values
        .get(&NodeId::GlyphShift(glyph_name.to_string()))
        .and_then(Value::as_num)
        .map_or(Affine::IDENTITY, |dx| Affine::translate((dx, 0.0)));

    for contour in &mut outline.contours {
        mg_geom::winding::orient_for_glyf(&mut contour.path, contour.role);
        contour.path.apply_affine(shift);
    }

    if !failed.contains(&NodeId::GlyphBbox(glyph_name.to_string())) {
        for component in &glyph.components {
            // A path component's contours are already the glyph's own.
            let Some(target) = &component.glyph else {
                continue;
            };
            if failed.contains(&NodeId::GlyphBbox(target.clone())) {
                continue;
            }
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            let Ok(transform) = component_affine(&mut ctx, component) else {
                continue;
            };
            outline.components.push(PlacedComponent {
                glyph: target.clone(),
                transform: shift * transform,
            });
        }
    }

    outline
}

/// Every contour a glyph renders, components decomposed: [`glyph_outline`]
/// for the glyph itself, plus each component's own sub-render placed by
/// its transform. "Stroked contours and component contours do not take
/// part in" fill nesting (spec §8.1), so each component's contours keep
/// the roles they had in their own glyph. This is `mg svg`'s preview;
/// always `Ok`, even if it ends up empty.
#[allow(clippy::result_unit_err)]
pub fn render_glyph(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    values: &IndexMap<NodeId, Value>,
    failed: &IndexSet<NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<(BezPath, ContourRole)>, ()> {
    let mut contours =
        render_glyph_at_depth(hir, instance, glyph_name, values, failed, diagnostics, 0);

    // Winding direction (spec §8.2) is decided again on the fully
    // transformed geometry: a component's own transform can carry a
    // reflection, which would flip an already-oriented contour's
    // direction right back out of convention.
    for (contour, role) in &mut contours {
        mg_geom::winding::orient_for_glyf(contour, *role);
    }
    Ok(contours)
}

fn render_glyph_at_depth(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    values: &IndexMap<NodeId, Value>,
    failed: &IndexSet<NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) -> Vec<(BezPath, ContourRole)> {
    // A font with a real component cycle already failed `mg-hir`'s
    // acyclic check, so this is purely a recursion guard for the preview,
    // not the user-facing depth error (that is `mg build`'s, spec §10.5).
    if depth >= COMPONENT_DEPTH {
        return Vec::new();
    }

    let outline = glyph_outline(hir, instance, glyph_name, values, failed, diagnostics);
    let mut contours: Vec<(BezPath, ContourRole)> = outline
        .contours
        .into_iter()
        .map(|c| (c.path, c.role))
        .collect();

    for component in outline.components {
        let sub = render_glyph_at_depth(
            hir,
            instance,
            &component.glyph,
            values,
            failed,
            diagnostics,
            depth + 1,
        );
        for (mut contour, role) in sub {
            contour.apply_affine(component.transform);
            contours.push((contour, role));
        }
    }

    contours
}
