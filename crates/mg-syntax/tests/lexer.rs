use mg_syntax::SyntaxKind::*;
use mg_syntax::lexer::tokenize;

fn kinds(source: &str) -> Vec<mg_syntax::SyntaxKind> {
    tokenize(source)
        .0
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| *k != WHITESPACE)
        .collect()
}

#[test]
fn angle_and_ratio_suffixes_produce_distinct_kinds() {
    assert_eq!(kinds("160deg"), vec![NUMBER_ANGLE, EOF]);
    assert_eq!(kinds("1.5rad"), vec![NUMBER_ANGLE, EOF]);
    assert_eq!(kinds("0.86%"), vec![NUMBER_RATIO, EOF]);
    assert_eq!(kinds("2em"), vec![NUMBER_RATIO, EOF]);
}

#[test]
fn whitespace_between_number_and_suffix_splits_tokens() {
    assert_eq!(kinds("160 deg"), vec![NUMBER, IDENT, EOF]);
}

#[test]
fn range_dots_are_one_token_distinct_from_member_dot() {
    assert_eq!(kinds("20..260"), vec![NUMBER, DOTDOT, NUMBER, EOF]);
    assert_eq!(kinds("p.bbox"), vec![IDENT, DOT, IDENT, EOF]);
}

/// `font`/`glyph`/`instance` are both declaration keywords and namespace
/// roots (spec §5.4, §5.10); the lexer always tokenizes them as keywords,
/// regardless of position. The parser treats these keywords as
/// identifier-like in expression position.
#[test]
fn namespace_root_keywords_lex_as_keywords_everywhere() {
    assert_eq!(kinds("glyph.bbox"), vec![GLYPH_KW, DOT, IDENT, EOF]);
}

#[test]
fn codepoint_literal() {
    assert_eq!(kinds("U+0041"), vec![NUMBER_CODEPOINT, EOF]);
}

#[test]
fn codepoint_above_max_is_flagged() {
    let (_, diagnostics) = tokenize("U+110000");
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn short_hex_run_falls_back_to_identifier() {
    // Only 3 hex digits: not a codepoint.
    assert_eq!(kinds("U+ABZ"), vec![IDENT, PLUS, IDENT, EOF]);
}

#[test]
fn line_comment_is_trivia_up_to_newline() {
    let (tokens, _) = tokenize("let x = 1; // comment\nlet y = 2;");
    let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&COMMENT));
}

#[test]
fn declaration_keywords_are_recognized() {
    assert_eq!(kinds("glyph"), vec![GLYPH_KW, EOF]);
    assert_eq!(kinds("close"), vec![CLOSE_KW, EOF]);
    assert_eq!(kinds("notakeyword"), vec![IDENT, EOF]);
}

/// Regression test for a real bug: `lex_number` used to compute a
/// suffixed number's end offset *before* consuming the suffix, so the
/// suffix was skipped over but never included in any token's span and
/// silently vanished from the token stream (and thus from round-tripped
/// text) entirely.
#[test]
fn suffixed_number_token_spans_the_whole_suffix() {
    let (tokens, _) = tokenize("152deg)");
    assert_eq!(tokens[0].kind, NUMBER_ANGLE);
    assert_eq!(tokens[0].text("152deg)"), "152deg");
}

#[test]
fn hex_integer_is_one_token() {
    assert_eq!(kinds("0x41"), vec![NUMBER_HEX, EOF]);
    assert_eq!(kinds("0xAb"), vec![NUMBER_HEX, EOF]);
}

#[test]
fn hex_integer_never_takes_a_suffix() {
    // Unlike a decimal number, hex integers don't consume `deg`/`rad`/
    // `em`/`%` — the suffix always lexes as a separate identifier.
    assert_eq!(kinds("0x41deg"), vec![NUMBER_HEX, IDENT, EOF]);
}

#[test]
fn hex_integer_out_of_range_is_flagged() {
    // 0xFFFFFFFFFFFFFF is far above 2^53.
    let (_, diagnostics) = tokenize("0xFFFFFFFFFFFFFF");
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn hex_prefix_with_no_digits_falls_back_to_identifier() {
    assert_eq!(kinds("0xzz"), vec![NUMBER, IDENT, EOF]);
}

#[test]
fn char_literal_is_one_token() {
    assert_eq!(kinds("'A'"), vec![NUMBER_CHAR, EOF]);
}

#[test]
fn char_literal_with_escape_has_no_diagnostics() {
    let (tokens, diagnostics) = tokenize(r"'\n'");
    assert_eq!(tokens[0].kind, NUMBER_CHAR);
    assert!(diagnostics.is_empty());
}

#[test]
fn char_literal_with_non_ascii_scalar_has_no_diagnostics() {
    let (tokens, diagnostics) = tokenize("'é'");
    assert_eq!(tokens[0].kind, NUMBER_CHAR);
    assert!(diagnostics.is_empty());
}

#[test]
fn empty_char_literal_is_flagged() {
    let (_, diagnostics) = tokenize("''");
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn char_literal_with_multiple_scalars_is_flagged() {
    assert_eq!(tokenize("'ab'").1.len(), 1);
    // A decomposed 'é' (e + combining acute) is two scalar values.
    assert_eq!(tokenize("'e\u{0301}'").1.len(), 1);
}

#[test]
fn char_literal_with_unknown_escape_is_flagged() {
    let (_, diagnostics) = tokenize(r"'\q'");
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn unterminated_char_literal_is_flagged() {
    let (_, diagnostics) = tokenize("'a");
    assert_eq!(diagnostics.len(), 1);
}
