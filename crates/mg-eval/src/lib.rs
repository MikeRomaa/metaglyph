//! Dependency graph, topological evaluation, cycle reporting, `Value`, and
//! the construction library (spec §4, §5.9–5.10). Calls `mg-geom` to
//! realize paths and compute `.bbox`.

pub mod construct;
pub mod errors;
pub mod eval;
pub mod graph;
pub mod toposort;
pub mod value;

pub use eval::{EvalOutcome, dirty_closure, evaluate, reevaluate};
pub use graph::{Graph, NodeId};
