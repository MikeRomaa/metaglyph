//! Snapshots `mg`'s rendered HIR diagnostics for each
//! `tests/diagnostics/*.mg` fixture (spec-plan M2), the same pattern as
//! `mg-syntax`'s corpus (spec-plan M0/M1) one level up the pipeline: these
//! fixtures parse cleanly, so every diagnostic here comes from
//! `mg_hir::lower` — name resolution, type checking, field validation, or
//! path structure (spec §13). Grown with every later milestone; M8 asserts
//! every code in the table has at least one case across both corpora.

use std::fs;
use std::path::Path;

use mg_syntax::ast::AstNode;

#[test]
fn diagnostics_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("reading {}: {err}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "mg"))
        .collect();
    entries.sort();
    assert!(
        !entries.is_empty(),
        "no .mg files found in {}",
        dir.display()
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(&dir);
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();

    for path in entries {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
        let parsed = mg_syntax::parse(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{} is meant to be syntactically clean but broken at the HIR level; got syntax diagnostics: {:?}",
            path.display(),
            parsed.diagnostics
        );

        let source_file = mg_syntax::ast::SourceFile::cast(parsed.syntax())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (_, diagnostics) = mg_hir::lower(&source_file);
        assert!(
            !diagnostics.is_empty(),
            "{} is meant to be broken, but produced no HIR diagnostics",
            path.display()
        );

        let filename = path.file_name().unwrap().to_string_lossy().into_owned();
        let rendered = diagnostics
            .iter()
            .map(|d| d.render(&filename, &source))
            .collect::<Vec<_>>()
            .join("\n");

        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        insta::assert_snapshot!(name, rendered);
    }
}
