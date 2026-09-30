//! The scripted LSP client shared by the integration tests: it drives
//! `mg_lsp::main_loop` over an in-memory connection.

#![allow(dead_code)] // Each test file uses a different subset.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::str::FromStr;
use std::thread::JoinHandle;
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Exit, Initialized, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Initialize, Request as _, Shutdown};
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    GeneralClientCapabilities, InitializeParams, InitializeResult, NumberOrString, Position,
    PositionEncodingKind, PublishDiagnosticsParams, TextDocumentContentChangeEvent,
    TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};

pub type ServerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub struct Client {
    connection: Connection,
    server: Option<JoinHandle<ServerResult>>,
    next_id: i32,
    /// Notifications that arrived while waiting for a response. The
    /// server publishes a second time once evaluation completes (plan 4,
    /// L3), at a moment no test controls, so every read goes through here.
    notifications: RefCell<VecDeque<Notification>>,
}

impl Client {
    /// Starts a server and completes the handshake, offering `encodings`
    /// (none at all when `None`).
    pub fn start(encodings: Option<Vec<PositionEncodingKind>>) -> (Self, InitializeResult) {
        Self::start_with(ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: encodings,
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    /// A client that accepts snippets in completions.
    pub fn start_with_snippets() -> (Self, InitializeResult) {
        Self::start_with(ClientCapabilities {
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                completion: Some(lsp_types::CompletionClientCapabilities {
                    completion_item: Some(lsp_types::CompletionItemCapability {
                        snippet_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    pub fn start_with(capabilities: ClientCapabilities) -> (Self, InitializeResult) {
        let (client, server) = Connection::memory();
        let handle = std::thread::spawn(move || mg_lsp::main_loop(server));
        let mut client = Client {
            connection: client,
            server: Some(handle),
            next_id: 0,
            notifications: RefCell::new(VecDeque::new()),
        };
        let params = InitializeParams {
            capabilities,
            ..Default::default()
        };
        let response = client.request(Initialize::METHOD, params);
        let result: InitializeResult =
            serde_json::from_value(response.response_result.unwrap()).unwrap();
        client.notify(Initialized::METHOD, lsp_types::InitializedParams {});
        (client, result)
    }

    pub fn request(&mut self, method: &str, params: impl serde::Serialize) -> Response {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).unwrap();
        loop {
            match self.recv() {
                Message::Response(response) => {
                    assert_eq!(response.id, id);
                    return response;
                }
                Message::Notification(n) => self.notifications.borrow_mut().push_back(n),
                other => panic!("expected a response, got {other:?}"),
            }
        }
    }

    pub fn notify(&self, method: &str, params: impl serde::Serialize) {
        let notification = Notification::new(method.to_string(), params);
        self.connection.sender.send(notification.into()).unwrap();
    }

    pub fn recv(&self) -> Message {
        self.connection
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the server answers")
    }

    /// Removes and returns the first buffered notification with `method`
    /// — one that arrived while a request was waiting for its response.
    pub fn take_notification(&self, method: &str) -> Option<Notification> {
        let mut buffered = self.notifications.borrow_mut();
        let index = buffered.iter().position(|n| n.method == method)?;
        buffered.remove(index)
    }

    /// The next `publishDiagnostics`, buffered or new.
    pub fn diagnostics(&self) -> PublishDiagnosticsParams {
        let notification = match self.notifications.borrow_mut().pop_front() {
            Some(n) => n,
            None => match self.recv() {
                Message::Notification(n) => n,
                other => panic!("expected publishDiagnostics, got {other:?}"),
            },
        };
        assert_eq!(notification.method, PublishDiagnostics::METHOD);
        serde_json::from_value(notification.params).unwrap()
    }

    /// The next `publishDiagnostics` for `uri` at `version`, skipping any
    /// for other documents or versions — such as a late evaluation
    /// publish for the version before.
    pub fn diagnostics_for(&self, uri: &Uri, version: i32) -> PublishDiagnosticsParams {
        loop {
            let published = self.diagnostics();
            if published.uri == *uri && published.version == Some(version) {
                return published;
            }
        }
    }

    /// The publish that joins `version`'s evaluation diagnostics to its
    /// static ones: the next one for that version after `open`/`change`
    /// consumed the first.
    pub fn evaluated(&self, uri: &Uri, version: i32) -> PublishDiagnosticsParams {
        self.diagnostics_for(uri, version)
    }

    pub fn open(&self, uri: &Uri, text: &str) -> PublishDiagnosticsParams {
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
        self.diagnostics_for(uri, 1)
    }

    pub fn change(&self, uri: &Uri, version: i32, text: &str) -> PublishDiagnosticsParams {
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
        self.diagnostics_for(uri, version)
    }

    /// `shutdown` then `exit`; the server thread must finish cleanly.
    pub fn stop(mut self) {
        let response = self.request(Shutdown::METHOD, ());
        assert!(response.response_result.is_ok(), "{response:?}");
        self.notify(Exit::METHOD, ());
        let result = self.server.take().unwrap().join().unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}

pub fn uri(name: &str) -> Uri {
    Uri::from_str(&format!("file:///fonts/{name}")).unwrap()
}

pub fn code(diagnostic: &lsp_types::Diagnostic) -> &str {
    match diagnostic.code.as_ref().unwrap() {
        NumberOrString::String(code) => code,
        NumberOrString::Number(_) => panic!("codes are strings"),
    }
}

pub const SAMPLE: &str = include_str!("../../../../tests/conformance.mg");

/// The position of the `n`th (0-based) occurrence of `needle` in `text`,
/// plus `into` bytes, for ASCII `text` (so bytes and UTF-16 units agree).
pub fn position_of(text: &str, needle: &str, n: usize, into: usize) -> Position {
    let offset = text
        .match_indices(needle)
        .nth(n)
        .unwrap_or_else(|| panic!("occurrence {n} of {needle:?}"))
        .0
        + into;
    let line = text[..offset].matches('\n').count();
    let column = offset - text[..offset].rfind('\n').map_or(0, |i| i + 1);
    Position::new(line as u32, column as u32)
}

/// The smallest header that makes a file a complete font (spec §5.6).
pub const PREAMBLE: &str = r#"font (name: "T", em: 1000)
metric baseline (y: 0, align: "bottom")
metric xHeight (y: 500)
metric capHeight (y: 700)
metric ascender (y: 740)
metric descender (y: -200, align: "bottom")
"#;

/// `body` after [`PREAMBLE`].
pub fn valid(body: &str) -> String {
    format!("{PREAMBLE}{body}\n")
}

/// `text` with its one `|` marker removed, and the marker's position.
pub fn cursor(text: &str) -> (String, Position) {
    let offset = text.find('|').expect("a `|` cursor marker");
    let clean = format!("{}{}", &text[..offset], &text[offset + 1..]);
    let line = text[..offset].matches('\n').count();
    let column = offset - text[..offset].rfind('\n').map_or(0, |i| i + 1);
    (clean, Position::new(line as u32, column as u32))
}
