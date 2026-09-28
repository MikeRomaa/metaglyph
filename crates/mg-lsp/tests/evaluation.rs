//! L3 (plan 4): evaluation off the keystroke path, per-instance
//! diagnostics, and evaluated values in hover.

mod common;

use common::{Client, SAMPLE, code, cursor, position_of, uri, valid};
use lsp_types::request::{HoverRequest, Request as _};
use lsp_types::{
    Hover, HoverContents, HoverParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkDoneProgressParams,
};

fn hover_at(client: &mut Client, file: &Uri, position: Position) -> Option<String> {
    let response = client.request(
        HoverRequest::METHOD,
        HoverParams {
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(file.clone()),
                position,
            ),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    );
    let hover: Option<Hover> = serde_json::from_value(response.response_result.unwrap()).unwrap();
    hover.map(|h| match h.contents {
        HoverContents::Markup(markup) => markup.value,
        other => panic!("expected markup, got {other:?}"),
    })
}

/// The part of a hover below its `---` rule: the evaluated values.
fn values(hover: &str) -> Option<&str> {
    hover.split_once("\n\n---\n\n").map(|(_, values)| values)
}

/// Opens `text`, waits for its evaluation, and returns the client.
fn evaluated(text: &str) -> (Client, Uri, lsp_types::PublishDiagnosticsParams) {
    let (client, _) = Client::start(None);
    let file = uri("eval.mg");
    client.open(&file, text);
    let published = client.evaluated(&file, 1);
    (client, file, published)
}

const INSTANCES: &str = r#"param k (default: 4, range: -10..10)
instance Regular ()
instance Neg (k: -4)
"#;

// -- diagnostics --------------------------------------------------------

#[test]
fn an_error_in_every_instance_is_published_once_listing_them() {
    let (client, _, published) = evaluated(&valid(&format!("{INSTANCES}let bad = sqrt(-1);")));
    let evaluated: Vec<_> = published
        .diagnostics
        .iter()
        .filter(|d| code(d) == "MG0603")
        .collect();
    assert_eq!(evaluated.len(), 1, "{:#?}", published.diagnostics);
    assert!(
        evaluated[0].message.ends_with("[Regular, Neg]"),
        "{}",
        evaluated[0].message
    );
    client.stop();
}

#[test]
fn an_error_in_one_instance_names_only_that_one() {
    let (client, _, published) = evaluated(&valid(&format!("{INSTANCES}let r = sqrt(k);")));
    let messages: Vec<&str> = published
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages, ["sqrt of a negative number (-4) [Neg]"]);
    client.stop();
}

#[test]
fn a_cycle_lists_every_hop_as_related_information() {
    let (client, file, published) =
        evaluated(&valid("let a = b + 1;\nlet b = c + 1;\nlet c = a + 1;"));
    let cycle = published
        .diagnostics
        .iter()
        .find(|d| code(d) == "MG0601")
        .unwrap_or_else(|| panic!("{:#?}", published.diagnostics));
    let related = cycle.related_information.as_ref().unwrap();
    assert_eq!(related.len(), 2);
    assert!(related.iter().all(|r| r.location.uri == file));
    client.stop();
}

#[test]
fn the_sample_reports_its_curvature_errors_per_instance() {
    let (client, _, published) = evaluated(SAMPLE);
    let curvature: Vec<&str> = published
        .diagnostics
        .iter()
        .filter(|d| code(d) == "MG0618")
        .map(|d| d.message.as_str())
        .collect();
    assert!(!curvature.is_empty());
    assert!(curvature.iter().all(|m| m.ends_with(']')), "{curvature:#?}");
    assert!(curvature.iter().any(|m| m.contains("Bold")));
    client.stop();
}

#[test]
fn only_the_latest_version_is_evaluated_and_a_broken_edit_drops_stale_results() {
    let (client, _) = Client::start(None);
    let file = uri("edits.mg");
    // Two quick edits: the first never gets evaluated on its own.
    client.open(&file, &valid("let bad = sqrt(-1);"));
    client.change(&file, 2, &valid("let bad = sqrt(-4);"));
    let published = client.evaluated(&file, 2);
    let messages: Vec<&str> = published
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages, ["sqrt of a negative number (-4) [Regular]"]);

    // A syntax error: only the syntax diagnostics, none of the old
    // evaluation's.
    let broken = client.change(&file, 3, &valid("let bad = sqrt(-4;"));
    assert!(
        broken
            .diagnostics
            .iter()
            .all(|d| code(d).starts_with("MG01")),
        "{:#?}",
        broken.diagnostics
    );
    client.stop();
}

// -- hover values -------------------------------------------------------

#[test]
fn hovering_a_let_shows_its_value_in_each_instance() {
    let (mut client, file, _) = evaluated(SAMPLE);
    let hover = hover_at(&mut client, &file, position_of(SAMPLE, "hair", 2, 0)).unwrap();
    assert!(hover.contains("let hair: num"), "{hover}");
    assert_eq!(values(&hover), Some("Regular 86 · Bold 128 · Condensed 86"));
    client.stop();
}

#[test]
fn zones_pairs_and_advances_show_in_their_own_forms() {
    let (mut client, file, _) = evaluated(SAMPLE);
    let hover = hover_at(&mut client, &file, position_of(SAMPLE, "capHeight.y", 0, 0)).unwrap();
    assert_eq!(
        values(&hover),
        Some(
            "- Regular: .y 700 / .ink 712\n- Bold: .y 700 / .ink 712\n- Condensed: .y 700 / .ink 712"
        )
    );
    let hover = hover_at(&mut client, &file, position_of(SAMPLE, "glyph A", 0, 6)).unwrap();
    let values = values(&hover).unwrap();
    assert!(values.starts_with("advance: Regular "), "{values}");
    assert!(values.contains(" · Bold "), "{values}");
    client.stop();
}

#[test]
fn a_failed_value_shows_its_error_and_an_anchor_its_pair() {
    let (text, position) = cursor(&valid(
        "glyph A (advance: 10) {\n  anchor t|op (at: (5, 700))\n}\nlet b|ad = sqrt(-1);",
    ));
    // `cursor` takes the first marker; find the second by hand.
    let text = text.replace("b|ad", "bad");
    let (mut client, file, _) = evaluated(&text);
    let hover = hover_at(&mut client, &file, position).unwrap();
    assert_eq!(values(&hover), Some("- Regular: (5, 700)"));
    let bad = position_of(&text, "bad", 0, 1);
    let hover = hover_at(&mut client, &file, bad).unwrap();
    assert_eq!(
        values(&hover),
        Some("- Regular: failed: sqrt of a negative number (-1)")
    );
    client.stop();
}

#[test]
fn an_alternate_glyphs_values_show_only_where_its_set_is_built() {
    let text = valid(
        "instance Upright ()
instance Italic (glyphset: Ital)
glyph a (advance: w) {
  let w = 500;
}
glyph a (glyphset: Ital, advance: w) {
  let w = 450;
}",
    );
    let (mut client, file, _) = evaluated(&text);
    let default = hover_at(&mut client, &file, position_of(&text, "let w", 0, 4)).unwrap();
    assert_eq!(values(&default), Some("Upright 500"));
    let alternate = hover_at(&mut client, &file, position_of(&text, "let w", 1, 4)).unwrap();
    assert_eq!(values(&alternate), Some("Italic 450"));
    client.stop();
}

#[test]
fn a_file_with_static_errors_is_not_evaluated() {
    let (mut client, _) = Client::start(None);
    let file = uri("static.mg");
    let text = valid("let a = 1;\nlet b = nope;");
    let published = client.open(&file, &text);
    assert!(published.diagnostics.iter().any(|d| code(d) == "MG0201"));
    // Long past the evaluation delay: still no values.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let hover = hover_at(&mut client, &file, position_of(&text, "let a", 0, 4)).unwrap();
    assert_eq!(values(&hover), None, "{hover}");
    client.stop();
}
