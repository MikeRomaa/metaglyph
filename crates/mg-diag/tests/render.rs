use mg_diag::{Code, Diagnostic, Label};

/// Exercises the renderer's full feature set (secondary label, help, note)
/// ahead of the M1 parser. `tests/diagnostics/*.mg` grows into the real
/// corpus once `mg-syntax` can produce these diagnostics itself.
#[test]
fn renders_unclosed_block_with_secondary_label_and_help() {
    let source = "glyph A (\n{\n  path p {\n";
    let diagnostic = Diagnostic::error(
        Code::new("MG0101"),
        "unexpected end of file inside block",
        Label::new(source.len()..source.len(), "expected `}` here"),
    )
    .with_secondary(Label::new(10..11, "unclosed block opened here"))
    .with_help("add a closing `}`")
    .with_note("blocks opened with `{` must be closed before the file ends");

    insta::assert_snapshot!(diagnostic.render("sample.mg", source));
}
