//! Dependency graph, topological evaluation, cycle reporting, `Value`, and
//! the construction library (spec §4, §5.9–5.10). Calls `mg-geom` to
//! realize paths, compute `.bbox`, and (`render`) render a whole glyph's
//! contours for `mg svg` (spec §6.4–§6.5, §8).

pub mod construct;
pub mod errors;
pub mod eval;
pub mod graph;
pub mod render;
pub mod toposort;
pub mod value;

pub use eval::{
    EvalOutcome, dirty_closure, eval_subexpr, eval_subexpr_with, evaluate, evaluate_cancellable,
    reevaluate,
};
pub use graph::{Graph, NodeId};
pub use render::{GlyphOutline, OutlineContour, PlacedComponent, glyph_outline, render_glyph};
