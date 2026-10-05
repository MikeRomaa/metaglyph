//! L1 (plan 4): stage-two diagnostics, document symbols, go-to-definition
//! for every spec §5.11 namespace, and references.

mod common;

use common::{Client, SAMPLE, code, position_of, uri, valid};
use lsp_types::request::{DocumentSymbolRequest, GotoDefinition, References, Request as _};
use lsp_types::{
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Location, PartialResultParams, Position, ReferenceContext,
    ReferenceParams, SymbolKind, TextDocumentIdentifier, TextDocumentPositionParams, Uri,
    WorkDoneProgressParams,
};

fn at(file: &Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(file.clone()), position)
}

fn definition(client: &mut Client, file: &Uri, position: Position) -> Option<Location> {
    let response = client.request(
        GotoDefinition::METHOD,
        GotoDefinitionParams {
            text_document_position_params: at(file, position),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    );
    let result: Option<GotoDefinitionResponse> =
        serde_json::from_value(response.response_result.unwrap()).unwrap();
    match result? {
        GotoDefinitionResponse::Scalar(location) => Some(location),
        other => panic!("expected one location, got {other:?}"),
    }
}

fn references(
    client: &mut Client,
    file: &Uri,
    position: Position,
    include_declaration: bool,
) -> Vec<Location> {
    let response = client.request(
        References::METHOD,
        ReferenceParams {
            text_document_position: at(file, position),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration,
            },
        },
    );
    let result: Option<Vec<Location>> =
        serde_json::from_value(response.response_result.unwrap()).unwrap();
    result.unwrap_or_default()
}

fn symbols(client: &mut Client, file: &Uri) -> Vec<DocumentSymbol> {
    let response = client.request(
        DocumentSymbolRequest::METHOD,
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(file.clone()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    );
    match serde_json::from_value(response.response_result.unwrap()).unwrap() {
        Some(DocumentSymbolResponse::Nested(symbols)) => symbols,
        other => panic!("expected nested symbols, got {other:?}"),
    }
}

/// Asserts that the name at occurrence `n` of `from` (plus `into` bytes)
/// jumps to the start of occurrence `m` of `to` (plus `to_into`).
fn assert_jump(
    client: &mut Client,
    file: &Uri,
    text: &str,
    (from, n, into): (&str, usize, usize),
    (to, m, to_into): (&str, usize, usize),
) {
    let location = definition(client, file, position_of(text, from, n, into))
        .unwrap_or_else(|| panic!("{from:?} #{n} resolves"));
    assert_eq!(location.uri, *file);
    assert_eq!(
        location.range.start,
        position_of(text, to, m, to_into),
        "{from:?} #{n} jumps to {to:?} #{m}"
    );
}

// -- diagnostics --------------------------------------------------------

#[test]
fn stage_two_diagnostics_are_published_with_the_syntax_ones() {
    let (client, _) = Client::start(None);
    let file = uri("names.mg");
    let published = client.open(&file, &valid("glyph A (advance: wdth) {}"));
    let codes: Vec<&str> = published.diagnostics.iter().map(code).collect();
    assert_eq!(codes, ["MG0201"], "{:#?}", published.diagnostics);
    client.stop();
}

#[test]
fn a_syntax_error_holds_back_stage_two_like_mg_check() {
    let (client, _) = Client::start(None);
    let file = uri("both.mg");
    let published = client.open(&file, &valid("glyph A (advance: wdth {}"));
    assert!(!published.diagnostics.is_empty());
    assert!(
        published
            .diagnostics
            .iter()
            .all(|d| code(d).starts_with("MG01")),
        "{:#?}",
        published.diagnostics
    );
    client.stop();
}

#[test]
fn a_file_without_a_font_reports_it_as_mg_check_does() {
    let (client, _) = Client::start(None);
    let published = client.open(&uri("lone.mg"), "glyph A (advance: 1) {}\n");
    assert!(
        published.diagnostics.iter().any(|d| code(d) == "MG0413"),
        "{:#?}",
        published.diagnostics
    );
    client.stop();
}

#[test]
fn a_duplicate_codepoint_is_reported_as_mg_check_does() {
    let (client, _) = Client::start(None);
    let text =
        valid("glyph A (codepoint: 'A', advance: 1) {}\nglyph B (codepoint: 'A', advance: 1) {}");
    let published = client.open(&uri("dup.mg"), &text);
    let codes: Vec<&str> = published.diagnostics.iter().map(code).collect();
    assert_eq!(codes, ["MG0705"]);
    client.stop();
}

// -- document symbols ---------------------------------------------------

#[test]
fn the_sample_outline_lists_every_declaration_in_order() {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);
    let symbols = symbols(&mut client, &file);

    let glyphs: Vec<&str> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::CLASS)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        glyphs,
        [
            "A", "B", "C", "D", "E", "F", "zero", "one", "two", "three", "four", "five", "six",
            "seven", "eight", "nine"
        ]
    );
    assert_eq!(symbols[0].name, "font");
    let params: Vec<&str> = symbols
        .iter()
        .filter(|s| s.detail.as_deref() == Some("param"))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        params,
        ["stem", "contrast", "sidebear", "capW", "figW", "barPos"]
    );
    assert_eq!(
        symbols
            .iter()
            .filter(|s| s.detail.as_deref() == Some("instance"))
            .count(),
        3
    );

    // Source order: every symbol starts after the one before it.
    let starts: Vec<Position> = symbols.iter().map(|s| s.range.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);

    // A glyph holds its lets and paths; the selection is its name.
    let c = symbols.iter().find(|s| s.name == "C").unwrap();
    assert_eq!(
        c.selection_range.start,
        position_of(SAMPLE, "glyph C", 0, 6)
    );
    let children = c.children.as_ref().unwrap();
    assert_eq!(children[0].name, "w");
    assert_eq!(children[0].kind, SymbolKind::VARIABLE);
    assert!(
        children
            .iter()
            .any(|s| s.name == "bowl" && s.kind == SymbolKind::FUNCTION)
    );
    client.stop();
}

#[test]
fn paths_hold_their_named_segments_and_kerns_read_left_to_right() {
    let (mut client, _) = Client::start(None);
    let file = uri("outline.mg");
    let text = valid(
        "glyph A (advance: 1) {
  anchor top (at: (0, 0))
  path p (stroke: 10) {
    start (at: (0, 0))
    line tip (to: (1, 0))
  }
}
group g (glyphs: [A])
kern (left: A, right: g, by: -5)",
    );
    client.open(&file, &text);
    let symbols = symbols(&mut client, &file);

    let a = symbols.iter().find(|s| s.name == "A").unwrap();
    let children = a.children.as_ref().unwrap();
    assert_eq!(children[0].name, "top");
    assert_eq!(children[0].kind, SymbolKind::PROPERTY);
    let p = &children[1];
    assert_eq!(p.name, "p");
    let segments = p.children.as_ref().unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].name, "tip");

    assert!(symbols.iter().any(|s| s.name == "kern A → g"));
    client.stop();
}

#[test]
fn the_outline_survives_a_syntax_error() {
    let (mut client, _) = Client::start(None);
    let file = uri("typing.mg");
    let text = valid("glyph A (advance: 1) {}\nglyph B (advance: \nglyph C (advance: 1) {}");
    let published = client.open(&file, &text);
    assert!(!published.diagnostics.is_empty());
    let names: Vec<String> = symbols(&mut client, &file)
        .into_iter()
        .filter(|s| s.kind == SymbolKind::CLASS)
        .map(|s| s.name)
        .collect();
    assert!(names.contains(&"A".to_string()), "{names:?}");
    client.stop();
}

// -- definition ---------------------------------------------------------

#[test]
fn sample_names_jump_to_their_declarations() {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);
    let text = SAMPLE;

    // A param, from inside an expression, and from the cursor's end.
    assert_jump(&mut client, &file, text, ("stem / 2", 0, 0), ("stem", 0, 0));
    assert_jump(&mut client, &file, text, ("stem / 2", 0, 4), ("stem", 0, 0));
    // A top-level let, and a metric through a member access.
    // (`hair` #0 is in a comment, #1 is its own declaration.)
    assert_jump(&mut client, &file, text, ("hair", 2, 0), ("let hair", 0, 4));
    assert_jump(
        &mut client,
        &file,
        text,
        ("capHeight.y", 0, 0),
        ("capHeight", 0, 0),
    );
    // A glyph-local let resolves in its own glyph, not the first `w`.
    let c_w = text.find("glyph C").unwrap();
    let w_use = text[c_w..].find("w * 0.93").unwrap() + c_w;
    let location = definition(
        &mut client,
        &file,
        position_of(text, &text[w_use..w_use + 8], 0, 0),
    )
    .unwrap();
    // `let w = capW;` #0 is glyph A's; #1 is glyph C's.
    let c_decl = text.match_indices("let w = capW;").nth(1).unwrap().0;
    assert!(c_decl > c_w, "occurrence #1 is inside glyph C");
    assert_eq!(
        location.range.start,
        position_of(text, "let w = capW;", 1, 4)
    );
    // A component's glyph, and `glyphs.X`.
    assert_jump(
        &mut client,
        &file,
        text,
        ("glyph: six", 0, 7),
        ("glyph six", 0, 6),
    );
    assert_jump(
        &mut client,
        &file,
        text,
        ("glyphs.six", 0, 7),
        ("glyph six", 0, 6),
    );
    // An instance override key names its param.
    assert_jump(
        &mut client,
        &file,
        text,
        ("(stem: 160", 0, 1),
        ("stem", 0, 0),
    );
    client.stop();
}

#[test]
fn every_reference_field_jumps_to_its_namespace() {
    let (mut client, _) = Client::start(None);
    let file = uri("refs.mg");
    let text = valid(
        r#"glyph A (advance: 10) {
  anchor top (at: (5, 700))
  path spine (stroke: 10, joinAt: { rise: "round" }) {
    start (at: (0, 0))
    line rise (to: (0, 700))
    line over (to: (10, 700))
  }
}
glyph A (glyphset: Alt, advance: 10) {}
glyph B (advance: 10) {
  anchor mark (at: glyphs.A.top)
  component (glyph: A)
}
group caps (glyphs: [A, B])
kern (left: B, right: caps, by: -5)
instance Italic (glyphset: Alt)"#,
    );
    client.open(&file, &text);

    assert_jump(
        &mut client,
        &file,
        &text,
        ("{ rise", 0, 2),
        ("line rise", 0, 5),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("glyphs.A.top", 0, 9),
        ("top", 0, 0),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("glyphs.A.top", 0, 7),
        ("glyph A", 0, 6),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("glyph: A", 0, 7),
        ("glyph A", 0, 6),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("[A, B]", 0, 4),
        ("glyph B", 0, 6),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("right: caps", 0, 7),
        ("group caps", 0, 6),
    );
    assert_jump(
        &mut client,
        &file,
        &text,
        ("left: B", 0, 6),
        ("glyph B", 0, 6),
    );
    // A glyph set names the first glyph declaring it.
    assert_jump(
        &mut client,
        &file,
        &text,
        ("(glyphset: Alt)", 0, 11),
        ("glyph A", 1, 6),
    );
    client.stop();
}

#[test]
fn builtins_and_namespaces_resolve_to_nothing() {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);
    for (needle, into) in [("polar(", 0), ("glyphs.six", 0), ("font (", 0)] {
        assert!(
            definition(&mut client, &file, position_of(SAMPLE, needle, 0, into)).is_none(),
            "{needle:?}"
        );
    }
    client.stop();
}

// -- references ---------------------------------------------------------

/// Occurrences of the word `word` outside `//` comments.
fn code_occurrences(text: &str, word: &str) -> usize {
    text.lines()
        .map(|line| line.split("//").next().unwrap())
        .map(|code| {
            code.match_indices(word)
                .filter(|&(i, _)| {
                    let before = code[..i].chars().next_back();
                    let after = code[i + word.len()..].chars().next();
                    let ident =
                        |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                    !ident(before) && !ident(after)
                })
                .count()
        })
        .sum()
}

#[test]
fn references_find_every_use_and_optionally_the_declaration() {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);

    let stem = position_of(SAMPLE, "stem", 0, 0);
    let with = references(&mut client, &file, stem, true);
    let without = references(&mut client, &file, stem, false);
    assert_eq!(with.len(), code_occurrences(SAMPLE, "stem"));
    assert_eq!(without.len(), with.len() - 1);
    assert!(without.iter().all(|l| l.range.start != stem));

    // Glyph C's own `w`: only the uses inside glyph C.
    let c = SAMPLE.find("glyph C").unwrap();
    let d = SAMPLE.find("glyph D").unwrap();
    // `let w = capW;` #0 is glyph A's; #1 is glyph C's.
    let w = position_of(SAMPLE, "let w = capW;", 1, 4);
    let refs = references(&mut client, &file, w, true);
    assert_eq!(refs.len(), code_occurrences(&SAMPLE[c..d], "w"));
    let (c_line, d_line) = (
        position_of(SAMPLE, "glyph C", 0, 0).line,
        position_of(SAMPLE, "glyph D", 0, 0).line,
    );
    assert!(
        refs.iter()
            .all(|l| (c_line..d_line).contains(&l.range.start.line))
    );
    client.stop();
}

#[test]
fn glyph_references_span_every_reference_form() {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);
    // `glyph six` itself, `glyphs.six.advance`, `component (glyph: six`,
    // and `glyphs.six.advance` again inside the transform.
    let six = position_of(SAMPLE, "glyph six", 0, 6);
    let refs = references(&mut client, &file, six, true);
    assert_eq!(refs.len(), 4, "{refs:#?}");
    assert_eq!(refs.len(), code_occurrences(SAMPLE, "six"));
    client.stop();
}
