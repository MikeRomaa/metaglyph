use mg_syntax::fmt::{FormatError, format, format_checked};

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
    assert_idempotent(include_str!("../../../tests/conformance.mg"));
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

/// Bodies always break onto multiple lines when non-empty, indented four
/// spaces per level: the formatter regenerates layout from structure
/// rather than preserving whether the source kept a short body on one line.
#[test]
fn reindents_nested_glyph_body() {
    let out = format(
        "glyph A (advance: 1) {\nlet w=1;\npath p (stroke: 1) {start (at: w) line (to: w)}\n}\n",
    );
    assert_eq!(
        out,
        "glyph A (advance: 1) {\n    let w = 1;\n    path p (stroke: 1) {\n        start (at: w)\n        line  (to: w)\n    }\n}\n"
    );
}

/// Inside a path body, every segment's `(` opens in one column, set by
/// the longest `keyword name`; `close` has no config and takes no part.
/// Only segments are aligned, not the paths around them.
#[test]
fn aligns_segment_configs_within_a_path() {
    let out = format(
        "glyph C (advance: 1) {\npath bowl (stroke: 1) {\nstart (at: a)\ncube (c2: c, to: d)\narc tip (center: e, to: f, sweep: \"cw\")\nclose\n}\npath p (stroke: 1) {\nstart (at: a)\nline (to: b)\n}\n}\n",
    );
    assert_eq!(
        out,
        "glyph C (advance: 1) {\n    path bowl (stroke: 1) {\n        start   (at: a)\n        cube    (c2: c, to: d)\n        arc tip (center: e, to: f, sweep: \"cw\")\n        close\n    }\n    path p (stroke: 1) {\n        start (at: a)\n        line  (to: b)\n    }\n}\n"
    );
    assert_idempotent(&out);
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
fn checked_formatting_matches_plain_formatting_when_nothing_is_lost() {
    let sample = include_str!("../../../tests/conformance.mg");
    assert_eq!(format_checked(sample), Ok(format(sample)));
    let messy = "let x=1; // note\n\n\n// lead\nlet y = x*2;\n";
    assert_eq!(format_checked(messy), Ok(format(messy)));
}

#[test]
fn checked_formatting_declines_a_file_with_syntax_errors() {
    assert_eq!(
        format_checked("glyph A (advance: 1 {}\n"),
        Err(FormatError::SyntaxErrors)
    );
}

/// The module's known gap: a comment between two config fields has no
/// place in the regenerated one-line config. The checked formatter must
/// refuse rather than drop it.
#[test]
fn checked_formatting_declines_rather_than_drop_a_comment() {
    let source = "param stem (\n  default: 100, // the regular weight\n  range: 20..260,\n)\n";
    assert!(!format(source).contains("regular weight"));
    assert_eq!(format_checked(source), Err(FormatError::WouldLoseText));
}

/// `param`s, `metric`s, and `instance`s declared together line up their
/// configs like a path's segments. A blank line or a declaration of
/// another kind starts a new group; a comment does not.
#[test]
fn aligns_configs_of_params_metrics_and_instances_declared_together() {
    let source = r#"param stem (default: 100) // weight
param contrast (default: 0.86)

param sidebear (default: 44)
param w (default: 1)
metric baseline (y: 0)
metric capHeight (y: 700)
let x = 1;
instance Regular ()
instance Bold (stem: 160)
"#;
    let expected = r#"param stem     (default: 100)  // weight
param contrast (default: 0.86)

param sidebear (default: 44)
param w        (default: 1)
metric baseline  (y: 0)
metric capHeight (y: 700)
let x = 1;
instance Regular ()
instance Bold    (stem: 160)
"#;
    assert_eq!(format(source), expected);
    assert_idempotent(source);
}

/// A config past 80 columns breaks one field per line, the first staying
/// after `(` and the rest aligned under it.
#[test]
fn a_long_config_breaks_one_field_per_line() {
    let source = r#"glyph a (advance: 1) {
path lower_bowl (stroke: 50, caps: "round", joins: "round") {
start (at: (0, 0))
line (to: (lower_bowl_ctr.x, 0))
arc (to: (lower_bowl_ctr.x, mid_y), rx: 0.486 * w, ry: 0.266 * h, sweep: "ccw", large: false)
line (to: (stem_x, mid_y))
}
}
"#;
    let expected = r#"glyph a (advance: 1) {
    path lower_bowl (stroke: 50, caps: "round", joins: "round") {
        start (at: (0, 0))
        line  (to: (lower_bowl_ctr.x, 0))
        arc   (to: (lower_bowl_ctr.x, mid_y),
               rx: 0.486 * w,
               ry: 0.266 * h,
               sweep: "ccw",
               large: false)
        line  (to: (stem_x, mid_y))
    }
}
"#;
    assert_eq!(format(source), expected);
    assert_idempotent(source);
    assert_eq!(format_checked(source), Ok(expected.to_string()));
}

/// The ` {` of a following body counts toward the width, and exactly 80
/// columns still fits.
#[test]
fn the_width_limit_counts_a_following_body_and_is_inclusive() {
    // `glyph a (advance: X, codepoint: 1)` is 33 + len(X) columns, and a
    // body's ` {` adds 2: 45 digits reach exactly 80.
    let line = |digits: usize| {
        format!(
            "glyph a (advance: {}, codepoint: 1) {{}}\n",
            "1".repeat(digits)
        )
    };
    let fits = line(45);
    assert_eq!(fits.find(')').unwrap() + 1 + " {".len(), 80);
    assert_eq!(format(&fits), fits);

    let breaks = line(46);
    assert_eq!(
        format(&breaks),
        format!(
            "glyph a (advance: {},\n         codepoint: 1) {{}}\n",
            "1".repeat(46)
        )
    );

    // A lone field has nowhere to break to, however long.
    let lone = format!("glyph a (advance: {}) {{}}\n", "1".repeat(100));
    assert_eq!(format(&lone), lone);
}

#[test]
fn config_fields_are_always_inline() {
    let out = format("param stem (\n  default: 100,\n  range: 20..260,\n)\n");
    assert_eq!(out, "param stem (default: 100, range: 20..260)\n");
}
