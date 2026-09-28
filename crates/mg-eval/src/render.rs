//! Whole-glyph rendering (spec §6.5, §8.1, §10.1): every rendering path's
//! contours, with the glyph's own filled contours' nesting resolved
//! across paths, plus every component's sub-render placed by its
//! transform. `crate::eval::render_path` is the per-path primitive this
//! builds on; this module is the glyph-level orchestration on top of it
//! that only a whole glyph's context (its other paths, its components)
//! can do.

use indexmap::{IndexMap, IndexSet};
use kurbo::BezPath;
use mg_diag::Diagnostic;
use mg_hir::model::{Hir, InstanceDecl};

use crate::eval::{self, EvalCtx, component_affine};
use crate::graph::{self, NodeId};
use crate::value::Value;
use mg_geom::winding::ContourRole;

/// `COMPONENT_DEPTH` (spec §14): a font compiled with a real component
/// cycle already failed `mg-hir`'s acyclic check, so this is purely a
/// recursion guard for `mg svg`'s own preview rendering, not a
/// user-facing depth error (that's M6/export's `mg build`, spec §10.5).
const MAX_COMPONENT_DEPTH: usize = 5;

/// Every contour a glyph renders (spec §6.5): its own paths' contours,
/// with `fill`-sourced ones re-scored for nesting (spec §8.1) across just
/// this glyph's own paths, plus each component's sub-render, transformed
/// and left otherwise alone ("stroked contours and component contours do
/// not take part in this count").
///
/// Unlike `.bbox` (whose `GlyphBbox` node depends on every one of its own
/// `PathBbox`es, so *one* broken path fails the whole glyph by
/// containment, spec §4.6), this is a preview: a path or component that
/// failed during `evaluate()` — `failed` is that same call's
/// `EvalOutcome::failed` — is skipped rather than aborting everything
/// else that render's fine. Its diagnostic was already produced by that
/// `evaluate()` call, so nothing is re-reported here; this always
/// returns `Ok`, even if it ends up empty.
#[allow(clippy::result_unit_err)]
pub fn render_glyph(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    values: &IndexMap<NodeId, Value>,
    failed: &IndexSet<NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<(BezPath, ContourRole)>, ()> {
    Ok(render_glyph_at_depth(
        hir,
        instance,
        glyph_name,
        values,
        failed,
        diagnostics,
        0,
    ))
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
    if depth >= MAX_COMPONENT_DEPTH {
        return Vec::new();
    }

    let glyph = graph::effective_glyph(hir, instance, glyph_name)
        .expect("mg-hir guarantees every alternate has a default");

    let mut contours: Vec<(BezPath, ContourRole)> = Vec::new();
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
        let is_filled = path.fill;
        let Ok(path_contours) =
            eval::render_path(hir, instance, glyph_name, path_index, values, diagnostics)
        else {
            continue;
        };
        for (i, entry) in path_contours.into_iter().enumerate() {
            // `render_path` always pushes a `fill`ed path's own contour
            // first, before any stroke contours or join patches.
            if is_filled && i == 0 {
                fill_slots.push(contours.len());
            }
            contours.push(entry);
        }
    }

    if fill_slots.len() > 1 {
        let fill_paths: Vec<BezPath> = fill_slots.iter().map(|&i| contours[i].0.clone()).collect();
        for (&slot, role) in fill_slots
            .iter()
            .zip(mg_geom::winding::resolve_fill_roles(&fill_paths))
        {
            contours[slot].1 = role;
        }
    }

    if !failed.contains(&NodeId::GlyphBbox(glyph_name.to_string())) {
        for component in &glyph.components {
            let Some(target) = &component.glyph else {
                continue;
            };
            if failed.contains(&NodeId::GlyphBbox(target.clone())) {
                continue;
            }
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            let Ok(affine) = component_affine(&mut ctx, component) else {
                continue;
            };
            let sub = render_glyph_at_depth(
                hir,
                instance,
                target,
                values,
                failed,
                diagnostics,
                depth + 1,
            );
            for (mut contour, role) in sub {
                contour.apply_affine(affine);
                contours.push((contour, role));
            }
        }
    }

    // Winding direction (spec §8.2) is decided once, on the fully
    // transformed geometry: a component's own transform can carry a
    // reflection, which would flip an already-oriented contour's
    // direction right back out of convention.
    if depth == 0 {
        for (contour, role) in &mut contours {
            mg_geom::winding::orient_for_glyf(contour, *role);
        }
    }

    contours
}
