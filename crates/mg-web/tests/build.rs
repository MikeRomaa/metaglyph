use mg_web::{analyze, build};

fn model(name: &str) -> mg_web::Model {
    let source =
        std::fs::read_to_string(format!("{}/../../samples/{name}", env!("CARGO_MANIFEST_DIR")))
            .expect("sample exists");
    analyze(&source, 0).1.expect("sample parses")
}

/// The `head` table's `created` field (seconds since 1904) in `data`.
fn head_created(data: &[u8]) -> i64 {
    let tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    let record = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&at| &data[at..at + 4] == b"head")
        .expect("a head table");
    let offset = u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize;
    i64::from_be_bytes(data[offset + 20..offset + 28].try_into().unwrap())
}

#[test]
fn a22x_builds_one_font_per_instance() {
    let model = model("a22x-mono.mg");
    let (fonts, diagnostics) = build(&model, 0);
    assert!(
        diagnostics.iter().all(|d| d.severity != "error"),
        "{diagnostics:?}"
    );
    assert_eq!(fonts.len(), model.hir.instances.len());
    assert_eq!(fonts[0].file_name, "A220Mono-Regular.ttf");
    assert_eq!(&fonts[0].data[..4], &[0, 1, 0, 0]);
}

#[test]
fn builds_are_reproducible_and_stamped() {
    let model = model("a22x-mono.mg");
    let (a, _) = build(&model, 1_700_000_000);
    let (b, _) = build(&model, 1_700_000_000);
    assert_eq!(a[0].data, b[0].data);
    // The Mac epoch (1904) is 2082844800 s before the Unix one.
    assert_eq!(head_created(&a[0].data), 1_700_000_000 + 2_082_844_800);
}
