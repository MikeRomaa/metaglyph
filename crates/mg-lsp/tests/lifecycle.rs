//! Round-trip tests for the L0 server (plan 4): a scripted client drives
//! `mg_lsp::main_loop` over an in-memory connection, through the whole
//! lifecycle — initialize, open, change, close, shutdown, exit.

mod common;

use common::{Client, SAMPLE, code, uri, valid};
use lsp_types::notification::{DidCloseTextDocument, Notification as _};
use lsp_types::{
    DidCloseTextDocumentParams, Position, PositionEncodingKind, TextDocumentIdentifier,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

#[test]
fn initialize_advertises_full_sync_and_utf16_by_default() {
    let (client, result) = Client::start(None);
    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF16)
    );
    assert_eq!(
        result.capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
    assert_eq!(result.server_info.unwrap().name, "mg");
    client.stop();
}

#[test]
fn utf8_is_chosen_whenever_the_client_offers_it() {
    let (client, result) = Client::start(Some(vec![
        PositionEncodingKind::UTF16,
        PositionEncodingKind::UTF8,
    ]));
    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF8)
    );
    client.stop();

    let (client, result) = Client::start(Some(vec![PositionEncodingKind::UTF32]));
    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF16)
    );
    client.stop();
}

#[test]
fn the_sample_opens_with_no_diagnostics() {
    let (client, _) = Client::start(None);
    let published = client.open(&uri("metaglyph-sans.mg"), SAMPLE);
    assert_eq!(published.uri, uri("metaglyph-sans.mg"));
    assert_eq!(published.version, Some(1));
    assert!(
        published.diagnostics.is_empty(),
        "{:#?}",
        published.diagnostics
    );
    client.stop();
}

#[test]
fn a_change_republishes_and_a_fix_clears() {
    let (client, _) = Client::start(None);
    let file = uri("a.mg");
    let opened = client.open(&file, &valid("glyph A (advance: 1) {}"));
    assert!(opened.diagnostics.is_empty(), "{:#?}", opened.diagnostics);

    let broken = client.change(&file, 2, &valid("glyph A (advance: 1 {}"));
    assert_eq!(broken.version, Some(2));
    assert!(!broken.diagnostics.is_empty());
    assert!(
        broken
            .diagnostics
            .iter()
            .all(|d| d.source.as_deref() == Some("mg"))
    );
    assert!(
        broken
            .diagnostics
            .iter()
            .all(|d| code(d).starts_with("MG01"))
    );

    let fixed = client.change(&file, 3, &valid("glyph A (advance: 1) {}"));
    assert_eq!(fixed.version, Some(3));
    assert!(fixed.diagnostics.is_empty());
    client.stop();
}

#[test]
fn an_unclosed_block_points_at_its_opener_in_related_information() {
    let (client, _) = Client::start(None);
    let file = uri("unclosed.mg");
    let text = "glyph A (advance: 1) {\n  let x = 1;\n";
    let published = client.open(&file, text);
    let unclosed = published
        .diagnostics
        .iter()
        .find(|d| code(d) == "MG0101")
        .unwrap_or_else(|| panic!("{:#?}", published.diagnostics));
    let related = unclosed.related_information.as_ref().unwrap();
    assert_eq!(related[0].location.uri, file);
    assert_eq!(related[0].location.range.start, Position::new(0, 21));
    client.stop();
}

#[test]
fn positions_follow_the_negotiated_encoding() {
    // A `// ══` rule and a `'é'` literal before the error: 3 + 1 bytes of
    // difference per `═`, 1 per `é`.
    let text = "// ══\nlet c = 'é' $;\n";
    let error_column = |encoding: Option<Vec<PositionEncodingKind>>| {
        let (client, _) = Client::start(encoding);
        let published = client.open(&uri("enc.mg"), text);
        let first = published.diagnostics.first().cloned();
        client.stop();
        first.unwrap_or_else(|| panic!("no diagnostic")).range.start
    };
    assert_eq!(error_column(None), Position::new(1, 12));
    assert_eq!(
        error_column(Some(vec![PositionEncodingKind::UTF8])),
        Position::new(1, 13)
    );
}

#[test]
fn closing_a_document_clears_its_diagnostics() {
    let (client, _) = Client::start(None);
    let file = uri("closing.mg");
    assert!(!client.open(&file, "glyph (").diagnostics.is_empty());
    client.notify(
        DidCloseTextDocument::METHOD,
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(file.clone()),
        },
    );
    let cleared = client.diagnostics();
    assert_eq!(cleared.uri, file);
    assert!(cleared.diagnostics.is_empty());
    client.stop();
}

#[test]
fn an_unsupported_request_gets_method_not_found() {
    let (mut client, _) = Client::start(None);
    let response = client.request("textDocument/codeAction", serde_json::json!({}));
    let error = response.response_result.unwrap_err();
    assert_eq!(error.code, lsp_server::ErrorCode::MethodNotFound as i32);
    client.stop();
}

#[test]
fn malformed_params_get_an_error_and_the_server_keeps_running() {
    let (mut client, _) = Client::start(None);
    let response = client.request("textDocument/hover", serde_json::json!({}));
    let error = response.response_result.unwrap_err();
    assert_eq!(error.code, lsp_server::ErrorCode::InvalidParams as i32);

    // A malformed notification is dropped, not fatal.
    client.notify("textDocument/didOpen", serde_json::json!({ "nonsense": 1 }));
    let published = client.open(&uri("after.mg"), &valid("glyph A (advance: 1) {}"));
    assert!(published.diagnostics.is_empty());
    client.stop();
}

#[test]
fn documents_are_independent() {
    let (client, _) = Client::start(None);
    let (a, b) = (uri("a.mg"), uri("b.mg"));
    assert!(!client.open(&a, "glyph (").diagnostics.is_empty());
    let published = client.open(&b, &valid("glyph B (advance: 1) {}"));
    assert_eq!(published.uri, b);
    assert!(published.diagnostics.is_empty());
    client.stop();
}
