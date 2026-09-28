//! `textDocument/documentSymbol` (plan 4, L1): the file's declarations
//! as a tree in source order — top-level declarations, and under each
//! glyph its `let`s, anchors, and paths, with each path's named segments
//! under it. The same hierarchy `outline.scm` gives Zed's outline panel,
//! but from the index, so names are exactly the resolver's.

use lsp_types::{DocumentSymbol, SymbolKind};

use crate::diagnostics::range;
use crate::index::{Decl, Index};
use crate::line_index::{Encoding, LineIndex};

pub struct Ctx<'a> {
    pub text: &'a str,
    pub lines: &'a LineIndex,
    pub encoding: Encoding,
}

#[allow(deprecated)] // `DocumentSymbol::deprecated` must be set to be constructed.
fn symbol(
    ctx: &Ctx,
    name: String,
    detail: &str,
    kind: SymbolKind,
    whole: &std::ops::Range<usize>,
    selection: &std::ops::Range<usize>,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: Some(detail.to_string()),
        kind,
        tags: None,
        deprecated: None,
        range: range(ctx.lines, ctx.text, whole, ctx.encoding),
        selection_range: range(ctx.lines, ctx.text, selection, ctx.encoding),
        children: (!children.is_empty()).then_some(children),
    }
}

fn leaf(ctx: &Ctx, decl: &Decl, detail: &str, kind: SymbolKind) -> (usize, DocumentSymbol) {
    let s = symbol(
        ctx,
        decl.name.clone(),
        detail,
        kind,
        &decl.range,
        &decl.name_range,
        Vec::new(),
    );
    (decl.range.start, s)
}

/// `symbols`, each keyed by its start offset, in source order.
fn in_order(mut symbols: Vec<(usize, DocumentSymbol)>) -> Vec<DocumentSymbol> {
    symbols.sort_by_key(|(start, _)| *start);
    symbols.into_iter().map(|(_, s)| s).collect()
}

pub fn document_symbols(index: &Index, ctx: &Ctx) -> Vec<DocumentSymbol> {
    let mut top = Vec::new();
    if let Some(font) = &index.font {
        top.push(leaf(ctx, font, "font", SymbolKind::MODULE));
    }
    top.extend(
        index
            .params
            .iter()
            .map(|d| leaf(ctx, d, "param", SymbolKind::CONSTANT)),
    );
    top.extend(
        index
            .metrics
            .iter()
            .map(|d| leaf(ctx, d, "metric", SymbolKind::CONSTANT)),
    );
    top.extend(
        index
            .lets
            .iter()
            .map(|d| leaf(ctx, d, "let", SymbolKind::VARIABLE)),
    );
    top.extend(
        index
            .instances
            .iter()
            .map(|d| leaf(ctx, d, "instance", SymbolKind::OBJECT)),
    );
    top.extend(
        index
            .groups
            .iter()
            .map(|d| leaf(ctx, d, "group", SymbolKind::ARRAY)),
    );

    for kern in &index.kerns {
        let side = |s: &Option<String>| s.clone().unwrap_or_else(|| "?".to_string());
        let name = format!("kern {} → {}", side(&kern.left), side(&kern.right));
        top.push((
            kern.range.start,
            symbol(
                ctx,
                name,
                "kern",
                SymbolKind::OPERATOR,
                &kern.range,
                &kern.range,
                Vec::new(),
            ),
        ));
    }

    for glyph in &index.glyphs {
        let mut children = Vec::new();
        children.extend(
            glyph
                .lets
                .iter()
                .map(|d| leaf(ctx, d, "let", SymbolKind::VARIABLE)),
        );
        children.extend(
            glyph
                .anchors
                .iter()
                .map(|d| leaf(ctx, d, "anchor", SymbolKind::PROPERTY)),
        );
        for path in &glyph.paths {
            let segments = path
                .segments
                .iter()
                .map(|d| leaf(ctx, d, "segment", SymbolKind::FIELD).1)
                .collect();
            let (name, selection) = match &path.decl {
                Some(d) => (d.name.clone(), d.name_range.clone()),
                None => ("(anonymous path)".to_string(), path.range.clone()),
            };
            children.push((
                path.range.start,
                symbol(
                    ctx,
                    name,
                    "path",
                    SymbolKind::FUNCTION,
                    &path.range,
                    &selection,
                    segments,
                ),
            ));
        }
        let detail = match &glyph.glyphset {
            Some(set) => format!("glyph, set {set}"),
            None => "glyph".to_string(),
        };
        top.push((
            glyph.decl.range.start,
            symbol(
                ctx,
                glyph.decl.name.clone(),
                &detail,
                SymbolKind::CLASS,
                &glyph.decl.range,
                &glyph.decl.name_range,
                in_order(children),
            ),
        ));
    }

    in_order(top)
}
