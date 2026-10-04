use mg_web::complete;

/// `source` with `|` marking the cursor, as a UTF-16 offset.
fn at(marked: &str) -> (String, usize) {
    let byte = marked.find('|').expect("a cursor mark");
    let offset = marked[..byte].encode_utf16().count();
    (marked.replacen('|', "", 1), offset)
}

fn labels(marked: &str, types: &mut Option<mg_lsp::types::NameTypes>) -> Vec<String> {
    let (source, offset) = at(marked);
    complete(&source, offset, types)
        .into_iter()
        .map(|c| c.label)
        .collect()
}

#[test]
fn top_level_offers_declaration_kinds() {
    let labels = labels("// ══ é ══\n|", &mut None);
    for kind in ["font", "glyph", "metric", "param"] {
        assert!(labels.iter().any(|l| l == kind), "{kind} in {labels:?}");
    }
}

#[test]
fn config_offers_missing_fields_after_non_ascii() {
    let labels = labels("// ══ é ══\nfont (name: \"Ü\", |)", &mut None);
    assert!(labels.iter().any(|l| l == "em"), "{labels:?}");
    assert!(!labels.iter().any(|l| l == "name"), "{labels:?}");
}

#[test]
fn members_use_types_from_the_last_text_that_lowered() {
    let mut types = None;
    labels("metric xHeight (y: 520)\nlet t = xHeight.y;|", &mut types);
    let labels = labels("metric xHeight (y: 520)\nlet t = xHeight.|", &mut types);
    assert!(labels.iter().any(|l| l == "y"), "{labels:?}");
}

#[test]
fn functions_insert_snippets() {
    let (source, offset) = at("let a = |");
    let items = complete(&source, offset, &mut None);
    let call = items
        .iter()
        .find(|c| c.kind == "function")
        .expect("a function");
    assert!(call.snippet, "{}", call.label);
    assert!(call.apply.as_deref().is_some_and(|a| a.contains("${1:")));
}
