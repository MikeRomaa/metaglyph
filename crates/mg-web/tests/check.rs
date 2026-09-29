use mg_web::check;

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../samples/{name}", env!("CARGO_MANIFEST_DIR")))
        .expect("sample exists")
}

#[test]
fn samples_parse_and_lower() {
    for name in ["a22x-mono.mg", "metaglyph-sans.mg"] {
        let state = check(&sample(name), 7);
        assert_eq!(state.version, 7);
        assert!(state.parse_ok, "{name}");
        assert!(state.glyph_count > 0);
        assert!(!state.instances.is_empty());
    }
}

#[test]
fn a22x_checks_clean() {
    let state = check(&sample("a22x-mono.mg"), 0);
    assert!(state.diagnostics.is_empty(), "{:?}", state.diagnostics);
}

#[test]
fn diagnostics_name_their_instances() {
    // metaglyph-sans has several instances; evaluation diagnostics are
    // tagged with the ones they fired in.
    let state = check(&sample("metaglyph-sans.mg"), 0);
    for d in state.diagnostics.iter().filter(|d| d.code.starts_with("MG06")) {
        assert!(d.message.ends_with(']'), "{}", d.message);
    }
}

#[test]
fn a22x_font_info() {
    let state = check(&sample("a22x-mono.mg"), 0);
    let font = state.font.expect("font info");
    assert_eq!(font.name.as_deref(), Some("A220 Mono"));
    assert_eq!(font.version, "2.000");
    assert_eq!(font.em, Some(1000));
    assert_eq!(state.instances, ["Regular"]);
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
