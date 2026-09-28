//! The server's main loop: the `initialize` handshake with encoding
//! negotiation, the document store, publishing diagnostics, and the
//! navigation requests.
//!
//! Synchronous by design (plan 4 decisions): Metaglyph files are small
//! and analysis is fast, so one thread handling one message at a time is
//! enough.

use indexmap::IndexMap;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    DocumentSymbolRequest, GotoDefinition, References, Request as RequestTrait,
};
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    InitializeParams, InitializeResult, Location, OneOf, Position, PositionEncodingKind,
    PublishDiagnosticsParams, ReferenceParams, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use mg_syntax::ast::AstNode;
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::diagnostics;
use crate::index::Index;
use crate::line_index::{Encoding, LineIndex};
use crate::symbols;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// One open document (full-document sync) and everything derived from
/// its text, recomputed on each change.
struct Document {
    text: String,
    version: i32,
    lines: LineIndex,
    root: SyntaxNode,
    index: Index,
    diagnostics: Vec<mg_diag::Diagnostic>,
}

impl Document {
    fn new(text: String, version: i32) -> Self {
        let lines = LineIndex::new(&text);
        let (root, diagnostics) = analyse(&text);
        let file = mg_syntax::ast::SourceFile::cast(root.clone())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let index = Index::new(&file);
        Self {
            text,
            version,
            lines,
            root,
            index,
            diagnostics,
        }
    }

    fn offset(&self, position: Position, encoding: Encoding) -> usize {
        self.lines.offset(&self.text, position, encoding)
    }

    fn range(&self, span: &std::ops::Range<usize>, encoding: Encoding) -> lsp_types::Range {
        diagnostics::range(&self.lines, &self.text, span, encoding)
    }

    /// The identifier at `offset`, preferring one that ends there (the
    /// cursor just after a name) over whatever starts there.
    fn ident_at(&self, offset: usize) -> Option<SyntaxToken> {
        let offset = offset.min(self.text.len()) as u32;
        self.root
            .token_at_offset(offset.into())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
}

/// Everything `mg check` reports for a single-file font, in the order it
/// reports it (plan 4, L1): the parse's diagnostics, then — when the
/// parse has no errors — lowering, name resolution, type checking, and
/// the static codepoint checks.
///
/// Like `mg check`, stage two is skipped after a syntax error: recovery
/// can misplace whole declarations, and lowering them would only restate
/// the same typo in unrelated-looking ways.
fn analyse(text: &str) -> (SyntaxNode, Vec<mg_diag::Diagnostic>) {
    let parsed = mg_syntax::parse(text);
    let root = parsed.syntax();
    let mut diagnostics = parsed.diagnostics.clone();
    if diagnostics
        .iter()
        .all(|d| d.severity != mg_diag::Severity::Error)
    {
        let file = mg_syntax::ast::SourceFile::cast(root.clone())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (hir, hir_diagnostics) = mg_hir::lower(&file);
        diagnostics.extend(hir_diagnostics);
        diagnostics.extend(mg_font::build::check_codepoints(&hir));
    }
    (root, diagnostics)
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
            document_symbol_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
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

    fn handle_request(&self, request: Request) -> Result<(), Error> {
        let id = request.id.clone();
        match request.method.as_str() {
            DocumentSymbolRequest::METHOD => {
                let params: DocumentSymbolParams = serde_json::from_value(request.params)?;
                let result = self.document_symbols(&params.text_document.uri);
                self.respond(id, result)
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = serde_json::from_value(request.params)?;
                let at = params.text_document_position_params;
                let result = self.definition(&at.text_document.uri, at.position);
                self.respond(id, result)
            }
            References::METHOD => {
                let params: ReferenceParams = serde_json::from_value(request.params)?;
                let at = params.text_document_position;
                let result = self.references(
                    &at.text_document.uri,
                    at.position,
                    params.context.include_declaration,
                );
                self.respond(id, result)
            }
            method => {
                let response = Response::new_err(
                    id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unsupported request: {method}"),
                );
                self.connection.sender.send(response.into())?;
                Ok(())
            }
        }
    }

    fn respond(&self, id: RequestId, result: impl serde::Serialize) -> Result<(), Error> {
        self.connection
            .sender
            .send(Response::new_ok(id, result).into())?;
        Ok(())
    }

    fn document_symbols(&self, uri: &Uri) -> Option<DocumentSymbolResponse> {
        let document = self.documents.get(uri.as_str())?;
        let ctx = symbols::Ctx {
            text: &document.text,
            lines: &document.lines,
            encoding: self.encoding,
        };
        Some(DocumentSymbolResponse::Nested(symbols::document_symbols(
            &document.index,
            &ctx,
        )))
    }

    fn definition(&self, uri: &Uri, position: Position) -> Option<GotoDefinitionResponse> {
        let document = self.documents.get(uri.as_str())?;
        let token = document.ident_at(document.offset(position, self.encoding))?;
        let def = document.index.resolve(&token)?;
        let target = document.index.definition(&def)?;
        Some(GotoDefinitionResponse::Scalar(Location::new(
            uri.clone(),
            document.range(&target, self.encoding),
        )))
    }

    fn references(
        &self,
        uri: &Uri,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let document = self.documents.get(uri.as_str())?;
        let token = document.ident_at(document.offset(position, self.encoding))?;
        let def = document.index.resolve(&token)?;
        let declaration = document.index.definition(&def);
        Some(
            document
                .index
                .references(&document.root, &def)
                .into_iter()
                .filter(|span| include_declaration || Some(span) != declaration.as_ref())
                .map(|span| Location::new(uri.clone(), document.range(&span, self.encoding)))
                .collect(),
        )
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

    /// Publishes `uri`'s diagnostics, all stages in one notification.
    fn publish(&self, uri: &Uri) -> Result<(), Error> {
        let Some(document) = self.documents.get(uri.as_str()) else {
            return Ok(());
        };
        let lsp_diagnostics = document
            .diagnostics
            .iter()
            .map(|d| diagnostics::to_lsp(d, uri, &document.text, &document.lines, self.encoding))
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
