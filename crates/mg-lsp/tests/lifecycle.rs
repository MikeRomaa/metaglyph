//! Round-trip tests for the L0 server (plan 4): a scripted client drives
//! `mg_lsp::main_loop` over an in-memory connection, through the whole
//! lifecycle — initialize, open, change, close, shutdown, exit.

use std::str::FromStr;
use std::thread::JoinHandle;
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{Initialize, Request as _, Shutdown};
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GeneralClientCapabilities, InitializeParams, InitializeResult,
    NumberOrString, Position, PositionEncodingKind, PublishDiagnosticsParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, VersionedTextDocumentIdentifier,
};

type ServerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

struct Client {
    connection: Connection,
    server: Option<JoinHandle<ServerResult>>,
    next_id: i32,
}

impl Client {
    /// Starts a server and completes the handshake, offering `encodings`
    /// (none at all when `None`).
    fn start(encodings: Option<Vec<PositionEncodingKind>>) -> (Self, InitializeResult) {
        let (client, server) = Connection::memory();
        let handle = std::thread::spawn(move || mg_lsp::main_loop(server));
        let mut client = Client {
            connection: client,
            server: Some(handle),
            next_id: 0,
        };
        let params = InitializeParams {
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: encodings,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let response = client.request(Initialize::METHOD, params);
        let result: InitializeResult =
            serde_json::from_value(response.response_result.unwrap()).unwrap();
        client.notify(Initialized::METHOD, lsp_types::InitializedParams {});
        (client, result)
    }

    fn request(&mut self, method: &str, params: impl serde::Serialize) -> Response {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).unwrap();
        match self.recv() {
            Message::Response(response) => {
                assert_eq!(response.id, id);
                response
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    fn notify(&self, method: &str, params: impl serde::Serialize) {
        let notification = Notification::new(method.to_string(), params);
        self.connection.sender.send(notification.into()).unwrap();
    }

    fn recv(&self) -> Message {
        self.connection
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the server answers")
    }

    fn diagnostics(&self) -> PublishDiagnosticsParams {
        match self.recv() {
            Message::Notification(n) if n.method == PublishDiagnostics::METHOD => {
                serde_json::from_value(n.params).unwrap()
            }
            other => panic!("expected publishDiagnostics, got {other:?}"),
        }
    }

    fn open(&self, uri: &Uri, text: &str) -> PublishDiagnosticsParams {
        self.notify(
            DidOpenTextDocument::METHOD,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "metaglyph".into(),
                    1,
                    text.into(),
                ),
            },
        );
        self.diagnostics()
    }

    fn change(&self, uri: &Uri, version: i32, text: &str) -> PublishDiagnosticsParams {
        self.notify(
            DidChangeTextDocument::METHOD,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.into(),
                }],
            },
        );
        self.diagnostics()
    }

    /// `shutdown` then `exit`; the server thread must finish cleanly.
    fn stop(mut self) {
        let response = self.request(Shutdown::METHOD, ());
        assert!(response.response_result.is_ok(), "{response:?}");
        self.notify(Exit::METHOD, ());
        let result = self.server.take().unwrap().join().unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}

fn uri(name: &str) -> Uri {
    Uri::from_str(&format!("file:///fonts/{name}")).unwrap()
}

fn code(diagnostic: &lsp_types::Diagnostic) -> &str {
    match diagnostic.code.as_ref().unwrap() {
        NumberOrString::String(code) => code,
        NumberOrString::Number(_) => panic!("codes are strings"),
    }
}

const SAMPLE: &str = include_str!("../../../samples/metaglyph-sans.mg");

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
    assert!(
        client
            .open(&file, "glyph A (advance: 1) {}\n")
            .diagnostics
            .is_empty()
    );

    let broken = client.change(&file, 2, "glyph A (advance: 1 {}\n");
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

    let fixed = client.change(&file, 3, "glyph A (advance: 1) {}\n");
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
    let response = client.request("textDocument/hover", serde_json::json!({}));
    let error = response.response_result.unwrap_err();
    assert_eq!(error.code, lsp_server::ErrorCode::MethodNotFound as i32);
    client.stop();
}

#[test]
fn documents_are_independent() {
    let (client, _) = Client::start(None);
    let (a, b) = (uri("a.mg"), uri("b.mg"));
    assert!(!client.open(&a, "glyph (").diagnostics.is_empty());
    let published = client.open(&b, "glyph B (advance: 1) {}");
    assert_eq!(published.uri, b);
    assert!(published.diagnostics.is_empty());
    client.stop();
}
