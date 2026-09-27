//! Snapshots `mg`'s rendered output for each `tests/diagnostics/*.mg`
//! fixture (spec-plan M0/M1): one small broken source per error code,
//! grown with every milestone. Each fixture is exercised by this one test
//! only (unlike `samples/*`, which are known-good files reused across
//! crates), so its `.snap` lives right next to it, not in a shared
//! `tests/snapshots/` directory. M8 asserts every code in the table has at
//! least one case here; this test only asserts each case actually fires.

use std::fs;
use std::path::Path;

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
            !parsed.diagnostics.is_empty(),
            "{} is meant to be broken, but produced no diagnostics",
            path.display()
        );
        // The CST is lossless even around `ERROR` nodes: broken source
        // must round-trip exactly, same as the conformance sample.
        assert_eq!(
            parsed.syntax().text().to_string(),
            source,
            "{} did not round-trip byte-for-byte",
            path.display()
        );

        let filename = path.file_name().unwrap().to_string_lossy().into_owned();
        let rendered = parsed
            .diagnostics
            .iter()
            .map(|d| d.render(&filename, &source))
            .collect::<Vec<_>>()
            .join("\n");

        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        insta::assert_snapshot!(name, rendered);
    }
}
