use mg_syntax::{SyntaxKind, parse};

fn assert_round_trip(source: &str) {
    let parsed = parse(source);
    assert_eq!(parsed.syntax().text().to_string(), source);
}

#[test]
fn round_trips_empty_source() {
    assert_round_trip("");
}

#[test]
fn round_trips_let_statement_with_comment() {
    assert_round_trip("// a comment\nlet hair = stem * contrast; // trailing\n");
}

#[test]
fn round_trips_glyph_with_path() {
    assert_round_trip(
        "glyph A (codepoint: U+0041, advance: glyph.bbox.x1 + sidebear) {\n  let w = capW;\n  path legL (stroke: stem) { start (at: al) line (to: lf) }\n}\n",
    );
}

#[test]
fn round_trips_param_with_range() {
    assert_round_trip("param stem (default: 100, range: 20..260)\n");
}

#[test]
fn round_trips_negative_range_bound() {
    assert_round_trip("param slantParam (default: 0, range: -10..10)\n");
}

#[test]
fn round_trips_hex_and_char_literals() {
    assert_round_trip("let a = 0x41;\nlet b = 'A';\n");
}

/// Hex, codepoint, and character integers are all just alternate number
/// spellings now (spec §5.1): `U+0041`, `0x41`, and `'A'` should each
/// parse as a plain `LITERAL`, same as a decimal number, with no
/// diagnostics.
#[test]
fn hex_codepoint_and_char_literals_parse_as_plain_literals() {
    for source in ["let a = 0x41;\n", "let a = U+0041;\n", "let a = 'A';\n"] {
        let parsed = parse(source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source:?}: {:?}",
            parsed.diagnostics
        );
        let root = parsed.syntax();
        assert!(
            root.descendants().any(|n| n.kind() == SyntaxKind::LITERAL),
            "{source:?} did not produce a LITERAL node"
        );
    }
}

#[test]
fn parses_param_with_zero_diagnostics() {
    let parsed = parse("param stem (default: 100, range: 20..260)\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn tuple_vs_paren_expr() {
    let parsed = parse("let a = (1, 2);\nlet b = (1);\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let text = format!("{:#?}", parsed.syntax());
    assert!(text.contains("TUPLE_EXPR"));
    assert!(text.contains("PAREN_EXPR"));
}

#[test]
fn power_binds_tighter_than_unary_minus() {
    // -2^2 == -(2^2): the BIN_EXPR (2^2) must nest inside the UNARY_EXPR.
    let parsed = parse("let a = -2^2;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let root = parsed.syntax();
    let unary = root
        .descendants()
        .find(|n| n.kind() == SyntaxKind::UNARY_EXPR)
        .expect("a UNARY_EXPR node");
    assert!(unary.children().any(|c| c.kind() == SyntaxKind::BIN_EXPR));
}

#[test]
fn power_right_operand_may_be_unary() {
    let parsed = parse("let a = 2^-1;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn unclosed_paren_reports_and_recovers() {
    let parsed = parse("param stem (default: 100\nparam other (default: 5)\n");
    assert!(!parsed.diagnostics.is_empty());
    // Recovery should still let the second declaration parse cleanly.
    let root = parsed.syntax();
    let params: Vec<_> = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::PARAM)
        .collect();
    assert_eq!(params.len(), 2);
}

#[test]
fn unclosed_brace_names_a_secondary_label_at_the_opener() {
    let parsed = parse("glyph A (advance: 1) {\nlet x = 1;\n");
    assert!(!parsed.diagnostics.is_empty());
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(diagnostic.code.as_str(), "MG0101");
    assert_eq!(diagnostic.secondary.len(), 1);
    assert!(diagnostic.secondary[0].message.contains("unclosed `{`"));
}

#[test]
fn unexpected_token_in_config_names_the_expected_set() {
    let parsed = parse("param stem (default: 100 deg, range: 20..260)\n");
    assert!(!parsed.diagnostics.is_empty());
    assert!(parsed.diagnostics[0].message.contains("expected"));
}

/// The plan's acceptance target: spec Appendix A parses with zero
/// diagnostics and round-trips byte-for-byte.
#[test]
fn conformance_sample_parses_with_zero_diagnostics_and_round_trips() {
    let source = include_str!("../../../samples/metaglyph-sans.mg");
    let parsed = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.syntax().text().to_string(), source);
}

/// Regression test for a real hang: a misspelled declaration keyword
/// (`glpyh` for `glyph`) isn't itself special-cased, so `declaration()`
/// falls into `recover_declaration`, which skips to the next `;`, `}`, or
/// declaration keyword — and stops *before* consuming that boundary,
/// trusting whichever `body()` call is waiting to close on it. At the top
/// level there is no such call and no matching `{`, so the swallowed `{`
/// leaves its `}` orphaned with nothing left to ever consume it.
/// `source_file`'s loop used to call `declaration()` unconditionally and
/// spun forever on that same unconsumed `}`. Runs the parse on a thread
/// with a hard timeout, so a regression fails fast instead of hanging the
/// whole test binary (and CI) the way the original bug did.
#[test]
fn orphaned_top_level_brace_does_not_hang() {
    let source = "glpyh C (advance: 1) {\n  let a = 1;\n}\n";
    let (tx, rx) = std::sync::mpsc::channel();
    let owned = source.to_string();
    std::thread::spawn(move || {
        let parsed = parse(&owned);
        let _ = tx.send(parsed.diagnostics.len());
    });
    let diagnostic_count = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("parse must terminate: an orphaned top-level `}` must not hang the parser");
    // One for the unrecognized `glpyh`, one for the now-orphaned `}`.
    assert_eq!(diagnostic_count, 2);
}
