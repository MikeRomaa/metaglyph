//! CST to HIR lowering, field schemas, enum validation, name resolution,
//! static type checking, and structural checks (spec §5.3–§5.8, §5.11).

pub mod const_eval;
pub mod model;
mod path_check;
mod resolve;
mod schema;
pub mod type_check;
pub mod types;

mod lower;

pub use model::Hir;

/// Lowers a parsed source file to its HIR (spec §5.3–§5.11), reporting
/// every field-validation, name-resolution, type, and path-structure
/// diagnostic (spec §13) it finds. Callers combine these with the
/// syntax diagnostics from `mg_syntax::parse` — this function assumes
/// nothing about whether the parse was clean, and lowers whatever tree it
/// is given.
pub fn lower(source_file: &mg_syntax::ast::SourceFile) -> (Hir, Vec<mg_diag::Diagnostic>) {
    lower::lower(source_file)
}
