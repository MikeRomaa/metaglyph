//! The server's main loop: the `initialize` handshake with encoding
//! negotiation, the document store, publishing diagnostics, and the
//! navigation requests.
//!
//! Synchronous by design (plan 4 decisions): Metaglyph files are small
//! and analysis is fast, so one thread handling one message at a time is
//! enough.

use std::time::{Duration, Instant};

use indexmap::IndexMap;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, Rename, Request as RequestTrait,
};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DocumentFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    Location, MarkupContent, MarkupKind, MessageType, OneOf, Position, PositionEncodingKind,
    PrepareRenameResponse, PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams,
    ServerCapabilities, ServerInfo, ShowMessageParams, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri, WorkspaceEdit,
};
use mg_syntax::ast::AstNode;
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::diagnostics;
use crate::evaluation::{self, InstanceResult};
use crate::index::Index;
use crate::line_index::{Encoding, LineIndex};
use crate::types::NameTypes;
use crate::{completion, hover, symbols};

/// How long edits must pause before evaluation starts (plan 4, L3).
const EVALUATION_DELAY: Duration = Duration::from_millis(300);

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
    uri: Uri,
    text: String,
    version: i32,
    lines: LineIndex,
    root: SyntaxNode,
    index: Index,
    diagnostics: Vec<mg_diag::Diagnostic>,
    /// From this version's type check, or carried over from the last
    /// version that had one (see `crate::types`).
    types: NameTypes,
    /// This version's HIR, when stages one and two found no errors —
    /// the only versions that are evaluated (plan 4, L3).
    hir: Option<mg_hir::Hir>,
    /// This version's evaluation, once it has run to completion.
    evaluation: Option<Vec<InstanceResult>>,
}

impl Document {
    fn new(uri: Uri, text: String, version: i32, previous: Option<NameTypes>) -> Self {
        let lines = LineIndex::new(&text);
        let (root, diagnostics, types, hir) = analyse(&text);
        let types = types.or(previous).unwrap_or_default();
        let file = mg_syntax::ast::SourceFile::cast(root.clone())
            .expect("SOURCE_FILE always casts from a parse's root node");
        let index = Index::new(&file);
        Self {
            uri,
            text,
            version,
            lines,
            root,
            index,
            diagnostics,
            types,
            hir,
            evaluation: None,
        }
    }

    /// Whether this version still needs evaluating.
    fn awaits_evaluation(&self) -> bool {
        self.hir.is_some() && self.evaluation.is_none()
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
/// when stage two ran, and the HIR only when neither stage found an error.
fn analyse(
    text: &str,
) -> (
    SyntaxNode,
    Vec<mg_diag::Diagnostic>,
    Option<NameTypes>,
    Option<mg_hir::Hir>,
) {
    let parsed = mg_syntax::parse(text);
    let root = parsed.syntax();
    let mut diagnostics = parsed.diagnostics.clone();
    let mut types = None;
    let mut clean_hir = None;
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
        if diagnostics
            .iter()
            .all(|d| d.severity != mg_diag::Severity::Error)
        {
            clean_hir = Some(hir);
        }
    }
    (root, diagnostics, types, clean_hir)
}

struct Server {
    connection: Connection,
    encoding: Encoding,
    /// Whether completions may use snippet syntax.
    snippets: bool,
    /// Keyed by URI string, in open order.
    documents: IndexMap<String, Document>,
    /// When the next evaluation may start: set [`EVALUATION_DELAY`] after
    /// each edit, and cleared once nothing awaits evaluation.
    evaluate_at: Option<Instant>,
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
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
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
        evaluate_at: None,
    };
    server.run()
}

impl Server {
    /// Handles messages, and evaluates documents whenever the client has
    /// been quiet since `evaluate_at`.
    fn run(&mut self) -> Result<(), Error> {
        loop {
            let message = match self.evaluate_at {
                None => match self.connection.receiver.recv() {
                    Ok(message) => message,
                    Err(_) => return Ok(()),
                },
                Some(at) => {
                    let wait = at.saturating_duration_since(Instant::now());
                    match self.connection.receiver.recv_timeout(wait) {
                        Ok(message) => message,
                        Err(err) if err.is_timeout() => {
                            self.evaluate_pending()?;
                            continue;
                        }
                        Err(_) => return Ok(()),
                    }
                }
            };
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
    }

    /// Evaluates every document awaiting it, publishing each one's
    /// diagnostics as it completes. Any incoming message cancels the run:
    /// the document stays pending, and evaluation resumes as soon as the
    /// client is quiet again — after the full delay if the message was an
    /// edit, since that resets `evaluate_at`.
    fn evaluate_pending(&mut self) -> Result<(), Error> {
        let receiver = self.connection.receiver.clone();
        let cancelled = || !receiver.is_empty();
        let pending: Vec<String> = self
            .documents
            .iter()
            .filter(|(_, d)| d.awaits_evaluation())
            .map(|(key, _)| key.clone())
            .collect();
        for key in pending {
            let document = &self.documents[&key];
            let hir = document.hir.as_ref().expect("awaiting evaluation");
            let Some(results) = evaluation::evaluate(hir, &cancelled) else {
                return Ok(());
            };
            let uri = document.uri.clone();
            self.documents[&key].evaluation = Some(results);
            self.publish(&uri)?;
        }
        self.evaluate_at = None;
        Ok(())
    }

    /// Answers `request`. Malformed params get an `InvalidParams` error
    /// response rather than stopping the server; only a closed connection
    /// is fatal.
    fn handle_request(&self, request: Request) -> Result<(), Error> {
        let Request { id, method, params } = request;
        // Rename can refuse with a reason the editor shows ("`x` is
        // already declared"), as a failed request rather than a result.
        if method == PrepareRenameRequest::METHOD || method == Rename::METHOD {
            let outcome = if method == Rename::METHOD {
                serde_json::from_value(params).map(|p: RenameParams| {
                    let at = p.text_document_position;
                    self.rename(&at.text_document.uri, at.position, &p.new_name)
                })
            } else {
                serde_json::from_value(params).map(|p: TextDocumentPositionParams| {
                    self.prepare_rename(&p.text_document.uri, p.position)
                })
            };
            let response = match outcome {
                Ok(Ok(value)) => Response::new_ok(id, value),
                Ok(Err(reason)) => Response::new_err(id, ErrorCode::RequestFailed as i32, reason),
                Err(err) => Response::new_err(
                    id,
                    ErrorCode::InvalidParams as i32,
                    format!("invalid params for {method}: {err}"),
                ),
            };
            self.connection.sender.send(response.into())?;
            return Ok(());
        }
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
            Formatting::METHOD => serde_json::from_value(params)
                .map(|p: DocumentFormattingParams| json(self.format(&p.text_document.uri))),
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

    /// `textDocument/formatting` (plan 4, L4): one whole-document edit
    /// from `mg fmt`, or none. The client's tab size and spacing options
    /// are ignored — `mg fmt` is canonical. No edits for a file with
    /// syntax errors, and none — with a message saying why — when
    /// formatting would lose a comment (see
    /// `mg_syntax::fmt::format_checked`): format-on-save must never
    /// delete text.
    fn format(&self, uri: &Uri) -> Option<Vec<TextEdit>> {
        use mg_syntax::fmt::{FormatError, format_checked};

        let document = self.documents.get(uri.as_str())?;
        match format_checked(&document.text) {
            Ok(formatted) if formatted == document.text => Some(Vec::new()),
            Ok(formatted) => {
                let whole = document.range(&(0..document.text.len()), self.encoding);
                Some(vec![TextEdit::new(whole, formatted)])
            }
            Err(FormatError::SyntaxErrors) => Some(Vec::new()),
            Err(FormatError::WouldLoseText) => {
                let params = ShowMessageParams {
                    typ: MessageType::WARNING,
                    message: "mg fmt: not formatting, because the result would change more \
                              than whitespace — typically a comment inside a `( … )` config, \
                              which the formatter can't place yet. Move it above the \
                              declaration."
                        .to_string(),
                };
                let notification = Notification::new(
                    <lsp_types::notification::ShowMessage as NotificationTrait>::METHOD.to_string(),
                    params,
                );
                // Losing the message is harmless; the edit list is still empty.
                let _ = self.connection.sender.send(notification.into());
                Some(Vec::new())
            }
        }
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
        let (mut value, span) = hover::hover(&ctx)?;

        // A name's evaluated value in each instance, once this version has
        // been evaluated.
        if let Some(results) = &document.evaluation
            && let Some(token) = document.ident_at(ctx.offset)
            && let Some(def) = document.index.resolve(&token)
            && let Some(values) = evaluation::hover_values(&document.index, results, &def)
        {
            value.push_str("\n\n---\n\n");
            value.push_str(&values);
        }
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

    /// The name at `position` and its range, if it can be renamed; `null`
    /// where there is no name at all.
    fn prepare_rename(&self, uri: &Uri, position: Position) -> Result<serde_json::Value, String> {
        let Some(document) = self.documents.get(uri.as_str()) else {
            return Ok(serde_json::Value::Null);
        };
        let Some(token) = document.ident_at(document.offset(position, self.encoding)) else {
            return Ok(serde_json::Value::Null);
        };
        mg_syntax::edit::renameable(&document.index, &token)?;
        let span: std::ops::Range<usize> = token.text_range().into();
        Ok(json(PrepareRenameResponse::RangeWithPlaceholder {
            range: document.range(&span, self.encoding),
            placeholder: token.text().to_string(),
        }))
    }

    /// The symbol at `position` renamed to `new_name`, with every
    /// reference, as one edit to this document — or why it can't be
    /// (spec §5.4, §5.11), via the same check the web editor uses.
    fn rename(
        &self,
        uri: &Uri,
        position: Position,
        new_name: &str,
    ) -> Result<serde_json::Value, String> {
        let document = self
            .documents
            .get(uri.as_str())
            .ok_or("This document isn't open.")?;
        let token = document
            .ident_at(document.offset(position, self.encoding))
            .ok_or("There is no name here to rename.")?;
        let edits =
            mg_syntax::edit::rename_symbol(&document.root, &document.index, &token, new_name)?;
        let edits = edits
            .into_iter()
            .map(|edit| TextEdit::new(document.range(&edit.range, self.encoding), edit.text))
            .collect();
        Ok(json(WorkspaceEdit {
            changes: Some([(uri.clone(), edits)].into_iter().collect()),
            ..Default::default()
        }))
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
                let document = Document::new(doc.uri.clone(), doc.text, doc.version, previous);
                self.documents.insert(key, document);
                self.publish(&doc.uri)?;
                self.schedule_evaluation();
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
                    let document = Document::new(
                        uri.clone(),
                        change.text,
                        params.text_document.version,
                        previous,
                    );
                    self.documents.insert(uri.as_str().to_string(), document);
                    self.publish(&uri)?;
                    self.schedule_evaluation();
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

    /// Evaluation starts once the client has been quiet for
    /// [`EVALUATION_DELAY`] after this edit.
    fn schedule_evaluation(&mut self) {
        self.evaluate_at = Some(Instant::now() + EVALUATION_DELAY);
    }

    /// Publishes `uri`'s diagnostics, all stages in one notification: the
    /// static ones at once, joined by the evaluation's when it completes.
    fn publish(&self, uri: &Uri) -> Result<(), Error> {
        let Some(document) = self.documents.get(uri.as_str()) else {
            return Ok(());
        };
        let evaluated = document
            .evaluation
            .as_deref()
            .map(evaluation::diagnostics)
            .unwrap_or_default();
        let lsp_diagnostics = document
            .diagnostics
            .iter()
            .chain(&evaluated)
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
