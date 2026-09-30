use mg_web::check;

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("sample exists")
}

#[test]
fn sample_parses_and_lowers() {
    let state = check(&sample("metaglyph-sans.mg"), 7);
    assert_eq!(state.version, 7);
    assert!(state.parse_ok);
    assert_eq!(state.glyph_count, 36);
    assert!(!state.instances.is_empty());
}

#[test]
fn sample_checks_clean() {
    let state = check(&sample("metaglyph-sans.mg"), 0);
    assert!(state.diagnostics.is_empty(), "{:?}", state.diagnostics);
}

#[test]
fn diagnostics_name_their_instances() {
    // metaglyph-sans has several instances; evaluation diagnostics are
    // tagged with the ones they fired in.
    let state = check(&sample("metaglyph-sans.mg"), 0);
    for d in state
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("MG06"))
    {
        assert!(d.message.ends_with(']'), "{}", d.message);
    }
}

#[test]
fn sample_font_info() {
    let state = check(&sample("metaglyph-sans.mg"), 0);
    let font = state.font.expect("font info");
    assert_eq!(font.name.as_deref(), Some("Metaglyph Sans"));
    assert_eq!(font.version, "1.000");
    assert_eq!(font.em, Some(1000));
    assert_eq!(state.instances, ["Light", "Regular", "Bold"]);
}

#[test]
fn syntax_error_is_not_parse_ok() {
    let state = check("glyph A (codepoint: 'A' {\n}", 1);
    assert!(!state.parse_ok);
    assert!(state.font.is_none());
    assert!(state.diagnostics.iter().any(|d| d.severity == "error"));
}

#[test]
fn diagnostic_offsets_are_utf16() {
    // `é` is 2 bytes but 1 UTF-16 unit, so the error after it must start
    // one unit earlier than its byte offset.
    let source = "// é\nglyph A (codepoint: 'A' {\n}";
    let state = check(source, 1);
    let first = &state.diagnostics[0];
    let byte = mg_syntax::parse(source).diagnostics[0].primary.span.start;
    assert_eq!(first.from, byte - 1);
}
