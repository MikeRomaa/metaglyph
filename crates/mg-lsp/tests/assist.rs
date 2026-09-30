//! L2 (plan 4): completion by cursor context, and static hover.

mod common;

use common::{Client, PREAMBLE, SAMPLE, cursor, position_of, uri, valid};
use lsp_types::request::{Completion, HoverRequest, Request as _};
use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, Documentation, Hover, HoverContents,
    HoverParams, InsertTextFormat, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkDoneProgressParams,
};

fn at(file: &Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(file.clone()), position)
}

fn complete_at(client: &mut Client, file: &Uri, position: Position) -> Vec<CompletionItem> {
    let response = client.request(
        Completion::METHOD,
        CompletionParams {
            text_document_position: at(file, position),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        },
    );
    match serde_json::from_value(response.response_result.unwrap()).unwrap() {
        Some(CompletionResponse::Array(items)) => items,
        other => panic!("expected an array, got {other:?}"),
    }
}

/// Opens `marked` (with its `|` cursor) as a fresh document and
/// completes at the cursor.
fn complete(marked: &str) -> Vec<CompletionItem> {
    let (mut client, _) = Client::start(None);
    let (text, position) = cursor(marked);
    let file = uri("complete.mg");
    client.open(&file, &text);
    let items = complete_at(&mut client, &file, position);
    client.stop();
    items
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

fn hover_at(client: &mut Client, file: &Uri, position: Position) -> Option<String> {
    let response = client.request(
        HoverRequest::METHOD,
        HoverParams {
            text_document_position_params: at(file, position),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    );
    let hover: Option<Hover> = serde_json::from_value(response.response_result.unwrap()).unwrap();
    hover.map(|h| match h.contents {
        HoverContents::Markup(markup) => markup.value,
        other => panic!("expected markup, got {other:?}"),
    })
}

/// The hover text at occurrence `n` of `needle` (plus `into`) in the
/// Appendix A sample.
fn hover_sample(needle: &str, n: usize, into: usize) -> Option<String> {
    let (mut client, _) = Client::start(None);
    let file = uri("sans.mg");
    client.open(&file, SAMPLE);
    let text = hover_at(&mut client, &file, position_of(SAMPLE, needle, n, into));
    client.stop();
    text
}

fn hover_marked(marked: &str) -> Option<String> {
    let (mut client, _) = Client::start(None);
    let (text, position) = cursor(marked);
    let file = uri("hover.mg");
    client.open(&file, &text);
    let hover = hover_at(&mut client, &file, position);
    client.stop();
    hover
}

const GLYPH: &str = r#"param stem (default: 100)
glyph A (advance: 500) {
  let half = stem / 2;
  anchor top (at: (250, 700))
  path spine (stroke: stem) {
    start (at: (0, 0))
    line rise (to: (0, 700))
    line over (to: (500, 700))
  }
}
glyph B (advance: 500) {}
glyph B (glyphset: Alt, advance: 500) {}
group round (glyphs: [A, B])
"#;

fn with_glyphs(tail: &str) -> String {
    valid(&format!("{GLYPH}{tail}"))
}

// -- field names --------------------------------------------------------

#[test]
fn a_config_offers_its_fields_with_types_and_docs() {
    let items = complete(&with_glyphs("glyph C (advance: 1) {\n  path p (|\n}"));
    assert_eq!(
        labels(&items),
        ["stroke", "fill", "caps", "joins", "joinAt"]
    );
    let joins = items.iter().find(|i| i.label == "joins").unwrap();
    assert_eq!(joins.detail.as_deref(), Some("string"));
    let Some(Documentation::MarkupContent(doc)) = &joins.documentation else {
        panic!("markdown documentation");
    };
    assert!(doc.value.contains("default `\"miter\"`"), "{}", doc.value);
    assert!(doc.value.contains("`\"bevel\"`"), "{}", doc.value);
    assert_eq!(joins.insert_text.as_deref(), Some("joins: "));
}

#[test]
fn fields_already_present_or_excluded_are_not_offered() {
    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p (stroke: 1, f|\n}",
    ));
    assert!(!labels(&items).contains(&"stroke"));
    assert!(labels(&items).contains(&"fill"));

    // `center` excludes `rx` and `ry` (spec §5.7).
    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p {\n    start (at: (0, 0))\n    arc (center: (1, 1), |\n  }\n}",
    ));
    assert_eq!(labels(&items), ["to", "sweep", "large"]);
}

#[test]
fn field_names_are_snippets_when_the_client_takes_them() {
    let (mut client, _) = Client::start_with_snippets();
    let (text, position) = cursor(&with_glyphs("glyph C (advance: 1) {\n  path p (|\n}"));
    let file = uri("snip.mg");
    client.open(&file, &text);
    let items = complete_at(&mut client, &file, position);
    let stroke = items.iter().find(|i| i.label == "stroke").unwrap();
    assert_eq!(stroke.insert_text.as_deref(), Some("stroke: $0"));
    assert_eq!(stroke.insert_text_format, Some(InsertTextFormat::SNIPPET));
    client.stop();
}

#[test]
fn an_instance_offers_its_fields_and_every_param() {
    let items = complete(&with_glyphs("instance Bold (|"));
    let labels = labels(&items);
    for field in [
        "slant",
        "glyphset",
        "styleName",
        "weightClass",
        "widthClass",
        "stem",
    ] {
        assert!(labels.contains(&field), "{field}: {labels:?}");
    }
}

// -- field values -------------------------------------------------------

#[test]
fn an_enum_value_is_offered_quoted_or_bare_inside_quotes() {
    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p (stroke: 1, joins: |)\n}",
    ));
    assert_eq!(labels(&items), ["\"miter\"", "\"round\"", "\"bevel\""]);

    // Typing the opening quote (plan 4's verification step 6).
    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p (stroke: 1, joins: \"|\n}",
    ));
    assert_eq!(labels(&items), ["miter", "round", "bevel"]);

    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p (stroke: 1, caps: (\"round\", \"|\"))\n}",
    ));
    assert_eq!(labels(&items), ["butt", "round", "square"]);

    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  path p {\n    start (at: (0, 0))\n    arc (to: (1, 1), center: (1, 0), sweep: \"|\")\n  }\n}",
    ));
    assert_eq!(labels(&items), ["ccw", "cw"]);
}

#[test]
fn reference_fields_offer_what_they_can_name() {
    // A component takes default-set glyphs: `B` once, not its alternate.
    let items = complete(&with_glyphs(
        "glyph C (advance: 1) {\n  component (glyph: |)\n}",
    ));
    assert_eq!(labels(&items), ["A", "B"]);
    // A kern side takes glyphs and groups.
    let items = complete(&with_glyphs("kern (left: |"));
    assert_eq!(labels(&items), ["A", "B", "round"]);
    let items = complete(&with_glyphs("group more (glyphs: [A, |"));
    assert_eq!(labels(&items), ["A", "B"]);
    let items = complete(&with_glyphs("instance I (glyphset: |)"));
    assert_eq!(labels(&items), ["Alt"]);
}

#[test]
fn join_at_offers_segments() {
    let marked = with_glyphs("").replace(
        "path spine (stroke: stem)",
        "path spine (stroke: stem, joinAt: { | })",
    );
    assert_eq!(labels(&complete(&marked)), ["rise", "over"]);

    let marked = with_glyphs("").replace(
        "path spine (stroke: stem)",
        "path spine (stroke: stem, joinAt: { over: | })",
    );
    assert_eq!(
        labels(&complete(&marked)),
        ["\"miter\"", "\"round\"", "\"bevel\""]
    );
}

// -- expressions and members --------------------------------------------

#[test]
fn an_expression_offers_scope_names_constants_namespaces_and_functions() {
    let marked = with_glyphs("").replace("let half = stem / 2;", "let half = |;");
    let items = complete(&marked);
    let labels = labels(&items);
    for name in [
        "half",
        "top",
        "spine",
        "stem",
        "capHeight",
        "up",
        "identity",
        "true",
        "math",
        "glyph",
        "glyphs",
        "polar",
        "lineThrough",
    ] {
        assert!(labels.contains(&name), "{name}: {labels:?}");
    }
    // Outside a glyph there is no `glyph` namespace or glyph scope.
    let items = complete(&with_glyphs("let x = |"));
    let labels = crate::labels(&items);
    assert!(!labels.contains(&"glyph") && !labels.contains(&"half"));
}

#[test]
fn functions_complete_as_snippets_with_their_signature() {
    let (mut client, _) = Client::start_with_snippets();
    let (text, position) = cursor(&with_glyphs("let x = po|"));
    let file = uri("fn.mg");
    client.open(&file, &text);
    let items = complete_at(&mut client, &file, position);
    let polar = items.iter().find(|i| i.label == "polar").unwrap();
    assert_eq!(
        polar.insert_text.as_deref(),
        Some("polar(${1:p}, ${2:len}, ${3:θ})")
    );
    assert_eq!(
        polar.detail.as_deref(),
        Some("polar(p: pair, len: num, θ: num) → pair")
    );
    client.stop();
}

#[test]
fn members_follow_the_receiver() {
    assert_eq!(
        labels(&complete(&with_glyphs("let x = glyphs.|"))),
        ["A", "B"]
    );
    assert_eq!(
        labels(&complete(&with_glyphs("let x = glyphs.A.|"))),
        ["advance", "bbox", "top"]
    );
    assert_eq!(
        labels(&complete(&with_glyphs("let x = font.|"))),
        ["name", "em", "version", "designer", "foundry", "license"]
    );
    assert_eq!(
        labels(&complete(&with_glyphs("let x = math.|"))),
        ["pi", "tau", "e"]
    );
}

#[test]
fn typed_members_use_the_last_good_type_check() {
    // `half.` is a syntax error, so this version is never type-checked;
    // the types come from the version before.
    let (mut client, _) = Client::start(None);
    let file = uri("typed.mg");
    client.open(&file, &with_glyphs(""));
    let (text, position) = cursor(&with_glyphs(
        "glyph C (advance: 1) {\n  let p = (1, 2);\n  let q = capHeight.|\n}",
    ));
    client.change(&file, 2, &text);
    let items = complete_at(&mut client, &file, position);
    assert_eq!(labels(&items), ["y", "ink", "overshoot"]);

    let (text, position) = cursor(&with_glyphs(
        "glyph C (advance: 1) {\n  let q = glyph.bbox.|\n}",
    ));
    client.change(&file, 3, &text);
    let items = complete_at(&mut client, &file, position);
    assert_eq!(
        labels(&items),
        ["x0", "y0", "x1", "y1", "width", "height", "center"]
    );
    client.stop();
}

// -- statements ---------------------------------------------------------

#[test]
fn a_statement_position_offers_the_declarations_legal_there() {
    assert_eq!(
        labels(&complete(&valid("gl|"))),
        [
            "font", "param", "metric", "let", "glyph", "instance", "group", "kern"
        ]
    );
    assert_eq!(
        labels(&complete(&valid("glyph C (advance: 1) {\n  pa|\n}"))),
        ["let", "path", "anchor", "component"]
    );
    assert_eq!(
        labels(&complete(&valid(
            "glyph C (advance: 1) {\n  path p {\n    start (at: (0, 0))\n    |\n  }\n}"
        ))),
        ["start", "line", "quad", "cube", "arc", "close"]
    );
}

#[test]
fn a_comment_offers_nothing() {
    assert!(complete(&format!("{PREAMBLE}// a note ab|\n")).is_empty());
}

// -- hover --------------------------------------------------------------

#[test]
fn a_name_shows_its_kind_type_and_declaration_line() {
    let text = hover_sample("stem / 2", 0, 1).unwrap();
    assert!(text.contains("param stem: num"), "{text}");
    // The declaration line as the sample spells it, whatever its layout.
    let (index, line) = SAMPLE
        .lines()
        .enumerate()
        .find(|(_, l)| l.starts_with("param stem "))
        .unwrap();
    assert!(
        text.contains(&format!("Declared on line {}:", index + 1)),
        "{text}"
    );
    assert!(text.contains(line.trim()), "{text}");

    let text = hover_sample("hair", 2, 0).unwrap();
    assert!(text.contains("let hair: num"), "{text}");

    let text = hover_sample("capHeight.y", 0, 0).unwrap();
    assert!(text.contains("metric capHeight: zone"), "{text}");
}

#[test]
fn glyph_scope_names_say_which_glyph() {
    let marked = with_glyphs("").replace(
        "anchor top (at: (250, 700))",
        "anchor top (at: (ha|lf, 700))",
    );
    let text = hover_marked(&marked).unwrap();
    assert!(text.contains("let half: num (in glyph A)"), "{text}");

    let marked = with_glyphs("").replace("path spine", "path sp|ine");
    let text = hover_marked(&marked).unwrap();
    assert!(text.contains("path spine: path (in glyph A)"), "{text}");

    let marked = with_glyphs("").replace("line rise", "line ri|se");
    let text = hover_marked(&marked).unwrap();
    assert!(text.contains("segment rise (in path spine)"), "{text}");
}

#[test]
fn a_function_shows_every_signature_and_its_meaning() {
    let text = hover_sample("polar(", 0, 1).unwrap();
    assert!(
        text.contains("polar(p: pair, len: num, θ: num) → pair"),
        "{text}"
    );
    assert!(text.contains("The point len from p at angle θ."), "{text}");

    let text = hover_marked(&with_glyphs("let t = sc|ale(2, 3);")).unwrap();
    assert!(text.contains("scale(s: num) → transform"), "{text}");
    assert!(text.contains("scale(num, num) → transform"), "{text}");
}

#[test]
fn a_field_name_shows_its_schema_entry() {
    let text = hover_sample("stroke: stem", 0, 1).unwrap();
    assert!(text.contains("stroke: num"), "{text}");
    assert!(text.contains("must be > 0"), "{text}");
    assert!(text.contains("Optional."), "{text}");

    let text = hover_sample("caps:", 0, 1).unwrap();
    assert!(text.contains("default `\"butt\"`"), "{text}");
    assert!(text.contains("`\"square\"`"), "{text}");

    let text = hover_sample("rsb:", 0, 1).unwrap();
    assert!(text.contains("rsb: num"), "{text}");
    assert!(
        text.contains("one or two of `advance`, `lsb`, `rsb`"),
        "{text}"
    );
}

#[test]
fn literals_show_their_converted_values() {
    let text = hover_sample("152deg", 0, 1).unwrap();
    assert_eq!(text, "`152deg` = 2.6529 rad");

    let text = hover_marked(&with_glyphs("let k = -0.0|5em;")).unwrap();
    assert_eq!(text, "`0.05em` = 50 units, at `font.em` = 1000");
    let text = hover_marked(&with_glyphs("let k = 2|5%;")).unwrap();
    assert_eq!(text, "`25%` = 0.25");

    let text = hover_sample("U+0043", 0, 2).unwrap();
    assert_eq!(text, "`U+0043` = 67 (U+0043)");
    let text = hover_marked(&with_glyphs("let c = 'é|';")).unwrap();
    assert_eq!(text, "`'é'` = 233 (U+00E9)");
    let text = hover_marked(&with_glyphs("let c = 0x1|F;")).unwrap();
    assert_eq!(text, "`0x1F` = 31");
}

#[test]
fn members_and_constants_show_their_types() {
    let text = hover_sample("capHeight.y", 0, 10).unwrap();
    assert!(text.contains("capHeight.y: num"), "{text}");
    let text = hover_marked(&with_glyphs("glyph C (advance: glyph.bb|ox.x1) {}")).unwrap();
    assert!(text.contains("glyph.bbox: rect"), "{text}");
    let text = hover_marked(&with_glyphs("let e = font.e|m;")).unwrap();
    assert!(text.contains("font.em: num"), "{text}");
    let text = hover_marked(&with_glyphs("let d = u|p;")).unwrap();
    assert!(text.contains("up: pair"), "{text}");
}

#[test]
fn glyphs_and_their_anchors_hover_through_glyphs_x() {
    let text = hover_sample("glyphs.six", 0, 8).unwrap();
    assert!(text.contains("glyph six"), "{text}");
    let text = hover_marked(&with_glyphs("let t = glyphs.A.to|p;")).unwrap();
    assert!(text.contains("anchor top: pair (in glyph A)"), "{text}");
}
