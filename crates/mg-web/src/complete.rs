//! Completion for the source editor: the LSP's completer (`mg_lsp::completion`),
//! with its items reshaped for CodeMirror's autocomplete.

use lsp_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat};
use mg_lsp::completion::{self, Ctx};
use mg_lsp::index::Index;
use mg_lsp::types::NameTypes;
use mg_syntax::ast::{AstNode, SourceFile};
use serde::Serialize;

use crate::offsets::Utf16Index;

/// One completion, in CodeMirror's terms.
#[derive(Serialize)]
pub struct Completion {
    pub label: String,
    /// CodeMirror's completion type: it picks the icon.
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub detail: Option<String>,
    /// Documentation, as Markdown.
    pub info: Option<String>,
    /// The text to insert, when it differs from the label.
    pub apply: Option<String>,
    /// Whether `apply` is a snippet (`${1:name}` placeholders).
    pub snippet: bool,
}

/// The completions at UTF-16 `offset` in `source`. `types` are the name
/// types of the last text that lowered: the text being typed usually
/// doesn't (`x.` always leaves an error), so they are kept across calls.
pub fn complete(source: &str, offset: usize, types: &mut Option<NameTypes>) -> Vec<Completion> {
    let parsed = mg_syntax::parse(source);
    let root = parsed.syntax();
    let file =
        SourceFile::cast(root.clone()).expect("SOURCE_FILE always casts from a parse's root node");
    if !parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == mg_diag::Severity::Error)
    {
        let (hir, _) = mg_hir::lower(&file);
        *types = Some(NameTypes::from_hir(&hir));
    }
    let index = Index::new(&file);
    let fallback = NameTypes::default();
    let ctx = Ctx {
        root: &root,
        index: &index,
        types: types.as_ref().unwrap_or(&fallback),
        offset: Utf16Index::new(source).to_byte(offset),
        snippets: true,
    };
    completion::complete(&ctx)
        .into_iter()
        .map(convert)
        .collect()
}

fn convert(item: CompletionItem) -> Completion {
    let snippet = item.insert_text_format == Some(InsertTextFormat::SNIPPET);
    let apply = item.insert_text.filter(|text| *text != item.label);
    Completion {
        kind: kind(item.kind),
        detail: item.detail,
        info: item.documentation.map(|d| match d {
            Documentation::String(s) => s,
            Documentation::MarkupContent(m) => m.value,
        }),
        snippet: snippet && apply.is_some(),
        apply,
        label: item.label,
    }
}

fn kind(kind: Option<CompletionItemKind>) -> &'static str {
    match kind {
        Some(CompletionItemKind::KEYWORD) => "keyword",
        Some(CompletionItemKind::FUNCTION) => "function",
        Some(CompletionItemKind::FIELD | CompletionItemKind::PROPERTY) => "property",
        Some(CompletionItemKind::CLASS) => "class",
        Some(CompletionItemKind::ENUM | CompletionItemKind::ENUM_MEMBER) => "enum",
        Some(CompletionItemKind::CONSTANT) => "constant",
        Some(CompletionItemKind::MODULE) => "namespace",
        _ => "variable",
    }
}
