//! The server's main loop: the `initialize` handshake with encoding
//! negotiation, the document store, and publishing diagnostics.
//!
//! Synchronous by design (plan 4 decisions): Metaglyph files are small
//! and analysis is fast, so one thread handling one message at a time is
//! enough.

use indexmap::IndexMap;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::{
    InitializeParams, InitializeResult, PositionEncodingKind, PublishDiagnosticsParams,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use crate::diagnostics;
use crate::line_index::{Encoding, LineIndex};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// One open document: its full text (full-document sync), the client's
/// version for it, and its line index.
struct Document {
    text: String,
    version: i32,
    index: LineIndex,
}

impl Document {
    fn new(text: String, version: i32) -> Self {
        let index = LineIndex::new(&text);
        Self {
            text,
            version,
            index,
        }
    }
}

struct Server {
    connection: Connection,
    encoding: Encoding,
    /// Keyed by URI string, in open order.
    documents: IndexMap<String, Document>,
}

/// UTF-8 when the client offers it, since that is what rowan's offsets
/// already are; UTF-16, the protocol's default, otherwise.
fn negotiate_encoding(params: &InitializeParams) -> Encoding {
    let offered = params
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref());
    match offered {
        Some(kinds) if kinds.contains(&PositionEncodingKind::UTF8) => Encoding::Utf8,
        _ => Encoding::Utf16,
    }
}

/// Performs the `initialize` handshake on `connection`, then serves it
/// until the client sends `shutdown` and `exit`.
pub fn main_loop(connection: Connection) -> Result<(), Error> {
    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let encoding = negotiate_encoding(&params);

    let result = InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(match encoding {
                Encoding::Utf8 => PositionEncodingKind::UTF8,
                Encoding::Utf16 => PositionEncodingKind::UTF16,
            }),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            ..Default::default()
        },
        server_info: Some(ServerInfo {
            name: "mg".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;

    let mut server = Server {
        connection,
        encoding,
        documents: IndexMap::new(),
    };
    server.run()
}

impl Server {
    fn run(&mut self) -> Result<(), Error> {
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => self.handle_notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    /// No request besides `shutdown` is supported yet.
    fn handle_request(&self, request: Request) -> Result<(), Error> {
        let response = Response::new_err(
            request.id,
            ErrorCode::MethodNotFound as i32,
            format!("unsupported request: {}", request.method),
        );
        self.connection.sender.send(response.into())?;
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<(), Error> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let doc = params.text_document;
                let key = doc.uri.as_str().to_string();
                self.documents
                    .insert(key, Document::new(doc.text, doc.version));
                self.publish(&doc.uri)?;
            }
            DidChangeTextDocument::METHOD => {
                let params: lsp_types::DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                // Full sync: the last change holds the whole new text.
                if let Some(change) = params.content_changes.into_iter().last() {
                    let uri = params.text_document.uri;
                    self.documents.insert(
                        uri.as_str().to_string(),
                        Document::new(change.text, params.text_document.version),
                    );
                    self.publish(&uri)?;
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.documents.shift_remove(uri.as_str());
                // A closed file's diagnostics would otherwise linger.
                self.send_diagnostics(uri, Vec::new(), None)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reparses `uri`'s document and publishes its syntax diagnostics.
    fn publish(&self, uri: &Uri) -> Result<(), Error> {
        let Some(document) = self.documents.get(uri.as_str()) else {
            return Ok(());
        };
        let parsed = mg_syntax::parse(&document.text);
        let lsp_diagnostics = parsed
            .diagnostics
            .iter()
            .map(|d| diagnostics::to_lsp(d, uri, &document.text, &document.index, self.encoding))
            .collect();
        self.send_diagnostics(uri.clone(), lsp_diagnostics, Some(document.version))
    }

    fn send_diagnostics(
        &self,
        uri: Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) -> Result<(), Error> {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        };
        let notification = Notification::new(PublishDiagnostics::METHOD.to_string(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }
}
