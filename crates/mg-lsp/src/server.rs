//! The server's main loop: the `initialize` handshake with encoding
//! negotiation, the document store, publishing diagnostics, and the
//! navigation requests.
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
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, References,
    Request as RequestTrait,
};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, Location,
    MarkupContent, MarkupKind, OneOf, Position, PositionEncodingKind, PublishDiagnosticsParams,
    ReferenceParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};
use mg_syntax::ast::AstNode;
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::diagnostics;
use crate::index::Index;
use crate::line_index::{Encoding, LineIndex};
use crate::types::NameTypes;
use crate::{completion, hover, symbols};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// A response payload. The LSP types always serialize.
fn json(value: impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).expect("LSP types serialize")
}

/// A notification's params, or `None` — logged to stderr, which is the
/// client's server log — when they are malformed. A notification has no
/// response to carry an error, and one bad message must not stop the
/// server.
fn notification_params<P: serde::de::DeserializeOwned>(notification: Notification) -> Option<P> {
    match serde_json::from_value(notification.params) {
        Ok(params) => Some(params),
        Err(err) => {
            eprintln!("mg lsp: invalid params for {}: {err}", notification.method);
            None
        }
    }
}

/// One open document (full-document sync) and everything derived from
/// its text, recomputed on each change.
struct Document {
    text: String,
    version: i32,
    lines: LineIndex,
    root: SyntaxNode,
    index: Index,
    diagnostics: Vec<mg_diag::Diagnostic>,
    /// From this version's type check, or carried over from the last
    /// version that had one (see `crate::types`).
    types: NameTypes,
}

impl Document {
    fn new(text: String, version: i32, previous: Option<NameTypes>) -> Self {
        let lines = LineIndex::new(&text);
        let (root, diagnostics, types) = analyse(&text);
        let types = types.or(previous).unwrap_or_default();
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
            types,
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
/// the same typo in unrelated-looking ways. The name types come back only
/// when stage two ran.
fn analyse(text: &str) -> (SyntaxNode, Vec<mg_diag::Diagnostic>, Option<NameTypes>) {
    let parsed = mg_syntax::parse(text);
    let root = parsed.syntax();
    let mut diagnostics = parsed.diagnostics.clone();
    let mut types = None;
    if diagnostics
        .iter()
        .all(|d| d.severity != mg_diag::Severity::Error)
    {
        let file = mg_syntax::ast::SourceFile::cast(root.clone())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let (hir, hir_diagnostics) = mg_hir::lower(&file);
        diagnostics.extend(hir_diagnostics);
        diagnostics.extend(mg_font::build::check_codepoints(&hir));
        types = Some(NameTypes::from_hir(&hir));
    }
    (root, diagnostics, types)
}

struct Server {
    connection: Connection,
    encoding: Encoding,
    /// Whether completions may use snippet syntax.
    snippets: bool,
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
    let snippets = params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|t| t.completion.as_ref())
        .and_then(|c| c.completion_item.as_ref())
        .and_then(|i| i.snippet_support)
        .unwrap_or(false);

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
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".into(), "\"".into()]),
                ..Default::default()
            }),
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
        snippets,
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

    /// Answers `request`. Malformed params get an `InvalidParams` error
    /// response rather than stopping the server; only a closed connection
    /// is fatal.
    fn handle_request(&self, request: Request) -> Result<(), Error> {
        let Request { id, method, params } = request;
        let result = match method.as_str() {
            DocumentSymbolRequest::METHOD => serde_json::from_value(params)
                .map(|p: DocumentSymbolParams| json(self.document_symbols(&p.text_document.uri))),
            GotoDefinition::METHOD => {
                serde_json::from_value(params).map(|p: GotoDefinitionParams| {
                    let at = p.text_document_position_params;
                    json(self.definition(&at.text_document.uri, at.position))
                })
            }
            References::METHOD => serde_json::from_value(params).map(|p: ReferenceParams| {
                let at = p.text_document_position;
                json(self.references(
                    &at.text_document.uri,
                    at.position,
                    p.context.include_declaration,
                ))
            }),
            Completion::METHOD => serde_json::from_value(params).map(|p: CompletionParams| {
                let at = p.text_document_position;
                json(self.completion(&at.text_document.uri, at.position))
            }),
            HoverRequest::METHOD => serde_json::from_value(params).map(|p: HoverParams| {
                let at = p.text_document_position_params;
                json(self.hover(&at.text_document.uri, at.position))
            }),
            _ => {
                let response = Response::new_err(
                    id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unsupported request: {method}"),
                );
                self.connection.sender.send(response.into())?;
                return Ok(());
            }
        };
        let response = match result {
            Ok(value) => Response::new_ok(id, value),
            Err(err) => Response::new_err(
                id,
                ErrorCode::InvalidParams as i32,
                format!("invalid params for {method}: {err}"),
            ),
        };
        self.connection.sender.send(response.into())?;
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

    fn completion(&self, uri: &Uri, position: Position) -> Option<CompletionResponse> {
        let document = self.documents.get(uri.as_str())?;
        let ctx = completion::Ctx {
            root: &document.root,
            index: &document.index,
            types: &document.types,
            offset: document.offset(position, self.encoding),
            snippets: self.snippets,
        };
        Some(CompletionResponse::Array(completion::complete(&ctx)))
    }

    fn hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        let document = self.documents.get(uri.as_str())?;
        let ctx = hover::Ctx {
            root: &document.root,
            text: &document.text,
            index: &document.index,
            types: &document.types,
            offset: document.offset(position, self.encoding),
        };
        let (value, span) = hover::hover(&ctx)?;
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(document.range(&span, self.encoding)),
        })
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
                let Some(params) =
                    notification_params::<lsp_types::DidOpenTextDocumentParams>(notification)
                else {
                    return Ok(());
                };
                let doc = params.text_document;
                let key = doc.uri.as_str().to_string();
                let previous = self.documents.get(&key).map(|d| d.types.clone());
                self.documents
                    .insert(key, Document::new(doc.text, doc.version, previous));
                self.publish(&doc.uri)?;
            }
            DidChangeTextDocument::METHOD => {
                let Some(params) =
                    notification_params::<lsp_types::DidChangeTextDocumentParams>(notification)
                else {
                    return Ok(());
                };
                // Full sync: the last change holds the whole new text.
                if let Some(change) = params.content_changes.into_iter().last() {
                    let uri = params.text_document.uri;
                    let previous = self.documents.get(uri.as_str()).map(|d| d.types.clone());
                    self.documents.insert(
                        uri.as_str().to_string(),
                        Document::new(change.text, params.text_document.version, previous),
                    );
                    self.publish(&uri)?;
                }
            }
            DidCloseTextDocument::METHOD => {
                let Some(params) =
                    notification_params::<lsp_types::DidCloseTextDocumentParams>(notification)
                else {
                    return Ok(());
                };
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
