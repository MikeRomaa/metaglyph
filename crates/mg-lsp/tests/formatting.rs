//! L4 (plan 4): `textDocument/formatting` from `mg fmt`.

mod common;

use common::{Client, SAMPLE, uri, valid};
use lsp_types::request::{Formatting, Request as _};
use lsp_types::{
    DocumentFormattingParams, FormattingOptions, Position, TextDocumentIdentifier, TextEdit, Uri,
    WorkDoneProgressParams,
};

fn format(client: &mut Client, file: &Uri) -> Vec<TextEdit> {
    let response = client.request(
        Formatting::METHOD,
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier::new(file.clone()),
            // Ignored: `mg fmt` is canonical.
            options: FormattingOptions {
                tab_size: 8,
                insert_spaces: false,
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    );
    let edits: Option<Vec<TextEdit>> =
        serde_json::from_value(response.response_result.unwrap()).unwrap();
    edits.expect("a document the server knows")
}

fn formatted(text: &str) -> (Vec<TextEdit>, Client) {
    let (mut client, _) = Client::start(None);
    let file = uri("fmt.mg");
    client.open(&file, text);
    (format(&mut client, &file), client)
}

#[test]
fn a_misformatted_file_gets_one_whole_document_edit() {
    let text = valid("let   w=1;\nglyph A (advance: w) {let x=w*2;}");
    let (edits, client) = formatted(&text);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, mg_syntax::fmt::format(&text));
    assert_eq!(edits[0].range.start, Position::new(0, 0));
    let lines = text.matches('\n').count() as u32;
    assert_eq!(edits[0].range.end, Position::new(lines, 0));
    client.stop();
}

#[test]
fn an_already_formatted_file_gets_no_edits() {
    let text = mg_syntax::fmt::format(SAMPLE);
    let (edits, client) = formatted(&text);
    assert!(edits.is_empty(), "{edits:#?}");
    // Empty because there is nothing to do, not because it was declined.
    assert!(client.take_notification("window/showMessage").is_none());
    client.stop();
}

#[test]
fn the_sample_formats_to_exactly_what_mg_fmt_writes() {
    let (edits, client) = formatted(SAMPLE);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, mg_syntax::fmt::format(SAMPLE));
    client.stop();
}

#[test]
fn a_file_with_syntax_errors_gets_no_edits() {
    let (edits, client) = formatted(&valid("glyph A (advance: 1 {}"));
    assert!(edits.is_empty());
    client.stop();
}

#[test]
fn formatting_that_would_drop_a_comment_is_declined_with_a_message() {
    let text = valid("param stem (\n  default: 100, // the regular weight\n  range: 20..260,\n)");
    let (edits, client) = formatted(&text);
    assert!(edits.is_empty(), "{edits:#?}");
    let message = client
        .take_notification("window/showMessage")
        .expect("a message saying why");
    let text = message.params["message"].as_str().unwrap();
    assert!(text.contains("comment"), "{text}");
    client.stop();
}
