use mg_syntax::fmt::format;

fn assert_idempotent(source: &str) {
    let once = format(source);
    let twice = format(&once);
    assert_eq!(
        once, twice,
        "formatting is not idempotent:\n---once---\n{once}\n---twice---\n{twice}"
    );
}

#[test]
fn idempotent_on_simple_declarations() {
    assert_idempotent("param stem (default: 100, range: 20..260)\n");
}

#[test]
fn idempotent_on_conformance_sample() {
    assert_idempotent(include_str!("../../../samples/metaglyph-sans.mg"));
}

#[test]
fn preserves_leading_comment_on_its_own_line() {
    let out = format("// header\nlet x = 1;\n");
    assert_eq!(out, "// header\nlet x = 1;\n");
}

#[test]
fn preserves_trailing_same_line_comment() {
    let out = format("let x = 1; // note\nlet y = 2;\n");
    assert_eq!(out, "let x = 1;  // note\nlet y = 2;\n");
}

#[test]
fn collapses_multiple_blank_lines_to_one() {
    let out = format("let x = 1;\n\n\n\nlet y = 2;\n");
    assert_eq!(out, "let x = 1;\n\nlet y = 2;\n");
}

/// Bodies always break onto multiple lines when non-empty: the formatter
/// regenerates layout from structure rather than preserving whether the
/// source kept a short body on one line.
#[test]
fn reindents_nested_glyph_body() {
    let out = format(
        "glyph A (advance: 1) {\nlet w=1;\npath p (stroke: 1) {start (at: w) line (to: w)}\n}\n",
    );
    assert_eq!(
        out,
        "glyph A (advance: 1) {\n  let w = 1;\n  path p (stroke: 1) {\n    start (at: w)\n    line (to: w)\n  }\n}\n"
    );
}

/// A blank line between a leading comment block and the very first
/// declaration in a file must survive, not just blank lines between two
/// later declarations.
#[test]
fn preserves_blank_line_after_leading_comment_on_first_item() {
    let out = format("// header\n\nfont (name: \"x\", em: 1000)\n");
    assert_eq!(out, "// header\n\nfont (name: \"x\", em: 1000)\n");
}

#[test]
fn config_fields_are_always_inline() {
    let out = format("param stem (\n  default: 100,\n  range: 20..260,\n)\n");
    assert_eq!(out, "param stem (default: 100, range: 20..260)\n");
}
