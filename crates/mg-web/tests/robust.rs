//! The engine must never panic on text a user can type: every deletion of
//! a run of characters from an inserted declaration (as when deleting it
//! by hand in the source pane) is analyzed and viewed without panicking.

use mg_web::{analyze, font_data, glyph_scene};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("sample exists")
}

fn survives(source: &str) {
    let result = std::panic::catch_unwind(|| {
        let (_, model) = analyze(source, 0);
        if let Some(model) = model {
            for instance in model.hir.instances.keys() {
                let data = font_data(&model, instance);
                for glyph in data.iter().flat_map(|d| &d.glyphs) {
                    glyph_scene(&model, instance, &glyph.name);
                }
            }
        }
    });
    assert!(result.is_ok(), "panicked on:\n{source}");
}

fn every_deletion_of(inserted: &str, anchor: &str) {
    let base = sample("metaglyph-sans.mg");
    let at = base.find(anchor).expect("anchor") + anchor.len();
    let source = format!("{}{inserted}{}", &base[..at], &base[at..]);
    survives(&source);
    let chars: Vec<(usize, char)> = inserted.char_indices().collect();
    for i in 0..chars.len() {
        for j in i + 1..=chars.len() {
            let from = at + chars[i].0;
            let to = at + chars.get(j).map_or(inserted.len(), |c| c.0);
            let text = format!("{}{}", &source[..from], &source[to..]);
            survives(&text);
        }
    }
}

#[test]
fn deleting_a_guide_by_hand_never_panics() {
    every_deletion_of(
        "\n    let l0 = hline(333);",
        "let footR = (x1 - dw / 2, 0);",
    );
}

#[test]
fn deleting_a_point_used_by_a_path_never_panics() {
    every_deletion_of("\n    let p0 = (10, 20);", "let footR = (x1 - dw / 2, 0);");
    // A point a path uses, removed: the path's reference dangles.
    let base = sample("metaglyph-sans.mg");
    survives(&base.replacen("    let footL = (x0 + dw / 2, 0);\n", "", 1));
}
