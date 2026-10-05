//! `textDocument/prepareRename` and `textDocument/rename`: the same
//! checked rename the web editor uses (`mg_syntax::edit::rename_symbol`).

mod common;

use common::{Client, cursor, uri, valid};
use lsp_types::request::{PrepareRenameRequest, Rename, Request as _};
use lsp_types::{
    OneOf, Position, PrepareRenameResponse, RenameOptions, RenameParams, TextDocumentIdentifier,
    TextDocumentPositionParams, TextEdit, Uri, WorkDoneProgressParams, WorkspaceEdit,
};

fn at(file: &Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(file.clone()), position)
}

/// `edits` applied to `text`, for ASCII `text`.
fn apply(text: &str, mut edits: Vec<TextEdit>) -> String {
    let offset = |p: Position| {
        let line_start: usize = text
            .split_inclusive('\n')
            .take(p.line as usize)
            .map(str::len)
            .sum();
        line_start + p.character as usize
    };
    edits.sort_by_key(|e| std::cmp::Reverse(offset(e.range.start)));
    let mut out = text.to_string();
    for edit in edits {
        out.replace_range(
            offset(edit.range.start)..offset(edit.range.end),
            &edit.new_text,
        );
    }
    out
}

/// Renames the symbol at the `|` in `marked` to `new_name`: the new text,
/// or the server's refusal.
fn rename(marked: &str, new_name: &str) -> Result<String, String> {
    let (text, position) = cursor(marked);
    let (mut client, _) = Client::start(None);
    let file = uri("rename.mg");
    client.open(&file, &text);
    let response = client.request(
        Rename::METHOD,
        RenameParams {
            text_document_position: at(&file, position),
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    );
    client.stop();
    let value = match response.response_result {
        Ok(value) => value,
        Err(error) => {
            assert_eq!(error.code, lsp_server::ErrorCode::RequestFailed as i32);
            return Err(error.message);
        }
    };
    let edit: WorkspaceEdit = serde_json::from_value(value).unwrap();
    // One document, one entry.
    let (changed, edits) = edit.changes.unwrap().into_iter().next().unwrap();
    assert_eq!(changed, file);
    Ok(apply(&text, edits))
}

#[test]
fn rename_is_advertised_with_prepare() {
    let (client, init) = Client::start(None);
    assert_eq!(
        init.capabilities.rename_provider,
        Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        }))
    );
    client.stop();
}

#[test]
fn prepare_names_the_symbol_or_nothing() {
    let (text, position) = cursor(&valid("let st|em = 10;\nglyph A (advance: stem) {}"));
    let (mut client, _) = Client::start(None);
    let file = uri("prepare.mg");
    client.open(&file, &text);

    let response = client.request(PrepareRenameRequest::METHOD, at(&file, position));
    let prepared: PrepareRenameResponse =
        serde_json::from_value(response.response_result.unwrap()).unwrap();
    let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = prepared else {
        panic!("{prepared:?}")
    };
    assert_eq!(placeholder, "stem");
    assert_eq!(range.end.character - range.start.character, 4);

    // Inside a number: nothing to rename.
    let (_, in_number) = cursor(&valid("let stem = 1|0;\nglyph A (advance: stem) {}"));
    let response = client.request(PrepareRenameRequest::METHOD, at(&file, in_number));
    assert_eq!(response.response_result.unwrap(), serde_json::Value::Null);
    client.stop();
}

#[test]
fn renaming_a_use_renames_the_declaration_and_every_use() {
    let renamed = rename(
        &valid("let stem = 10;\nglyph A (advance: st|em) {\n  let w = stem * 2;\n}"),
        "thick",
    )
    .unwrap();
    assert_eq!(
        renamed,
        valid("let thick = 10;\nglyph A (advance: thick) {\n  let w = thick * 2;\n}")
    );
}

#[test]
fn renaming_a_glyph_renames_its_references() {
    let renamed = rename(
        &valid("glyph o| (advance: 10) {}\nglyph A (advance: glyphs.o.advance) { component (glyph: o) }"),
        "oh",
    )
    .unwrap();
    assert_eq!(
        renamed,
        valid(
            "glyph oh (advance: 10) {}\nglyph A (advance: glyphs.oh.advance) { component (glyph: oh) }"
        )
    );
}

#[test]
fn a_rename_that_would_collide_or_is_not_a_name_is_refused() {
    let source = "let stem = 10;\nglyph A (advance: stem) {\n  let |w = stem * 2;\n}";
    let shadow = rename(&valid(source), "stem").unwrap_err();
    assert!(shadow.contains("would shadow"), "{shadow}");
    let bad = rename(&valid(source), "2w").unwrap_err();
    assert!(bad.contains("not an identifier"), "{bad}");
    let reserved = rename(&valid(source), "glyph").unwrap_err();
    assert!(reserved.contains("reserved"), "{reserved}");
}
