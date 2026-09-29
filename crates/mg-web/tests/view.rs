use mg_web::{analyze, font_data, glyph_scene};

fn model(name: &str) -> mg_web::Model {
    let source =
        std::fs::read_to_string(format!("{}/../../samples/{name}", env!("CARGO_MANIFEST_DIR")))
            .expect("sample exists");
    analyze(&source, 0).1.expect("sample parses")
}

#[test]
fn font_data_for_a22x() {
    let model = model("a22x-mono.mg");
    let data = font_data(&model, "Regular").expect("instance exists");
    assert_eq!(data.em, 1000.0);

    let names: Vec<_> = data.metrics.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["baseline", "xHeight", "capHeight", "ascender", "descender"]);
    let cap = &data.metrics[2];
    assert_eq!(cap.expr, "h");
    assert_eq!(cap.y, Some(1000.0));

    let lets: Vec<_> = data
        .lets
        .iter()
        .map(|l| (l.name.as_str(), l.expr.as_str(), l.value.as_str()))
        .collect();
    assert_eq!(lets[..3], [("h", "1em", "1000"), ("w", "0.5 * h", "500"), ("s", "h / 1.2", "833.3")]);

    let a = data.glyphs.iter().find(|g| g.name == "A").expect("glyph A");
    assert_eq!(a.codepoints, [0x41]);
    assert_eq!(a.fields.advance.as_deref(), Some("s"));
    assert!((a.advance.unwrap() - 1000.0 / 1.2).abs() < 1e-9);
    assert!(a.ink.is_some());
    assert!(a.outline.starts_with('M'));
    assert_eq!(a.errors, 0);
}

#[test]
fn unknown_instance_is_none() {
    let model = model("a22x-mono.mg");
    assert!(font_data(&model, "Nope").is_none());
    assert!(glyph_scene(&model, "Nope", "A").is_none());
    assert!(glyph_scene(&model, "Regular", "Nope").is_none());
}

#[test]
fn glyph_a_scene() {
    let model = model("a22x-mono.mg");
    let scene = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert!(!scene.outline.is_empty());

    let names: Vec<_> = scene.paths.iter().map(|p| p.name.as_deref()).collect();
    assert_eq!(names, [Some("stem"), Some("bar")]);
    let stem = &scene.paths[0];
    assert_eq!(stem.stroke.as_deref(), Some("50"));
    assert_eq!(stem.joins, "round");
    let refs: Vec<_> = stem.segments.iter().map(|s| s.to_ref.as_deref()).collect();
    assert_eq!(refs, [Some("stem0"), Some("stem1"), Some("stem2")]);
    assert_eq!(stem.segments[1].to, Some([250.0, 1000.0]));

    let point = |name: &str| scene.points.iter().find(|p| p.name == name).expect(name);
    assert_eq!(point("stem1").role, "skeleton");
    assert_eq!(point("stem1").expr, "(0.500 * w, h)");
    assert_eq!(point("bar0").role, "skeleton");
    assert_eq!(point("bar0").callee.as_deref(), Some("meet"));

    let bar_y = scene.lines.iter().find(|l| l.name == "bar_y").expect("bar_y");
    assert_eq!(bar_y.expr, "hline(0.333 * h)");
    assert!((bar_y.p0[1] - 333.0).abs() < 1e-9);

    // Spans point at the declaration text.
    let source = &model.source;
    let [from, to] = point("stem1").span;
    assert_eq!(&source[from..to], "let stem1 = (0.500 * w, h);");
}

#[test]
fn curve_controls_are_reported() {
    let model = model("metaglyph-sans.mg");
    let data = font_data(&model, "Regular").expect("Regular");
    let found = data.glyphs.iter().any(|g| {
        glyph_scene(&model, "Regular", &g.name)
            .expect("scene")
            .paths
            .iter()
            .flat_map(|p| &p.segments)
            .any(|s| s.kind == "cube" && s.controls.len() == 2)
    });
    assert!(found, "some glyph in metaglyph-sans has a cube with controls");
}

#[test]
fn polar_points_get_a_ray_from_their_origin() {
    let model = model("a22x-mono.mg");
    let scene = glyph_scene(&model, "Regular", "a").expect("glyph a");
    let ray = scene
        .lines
        .iter()
        .find(|l| l.of.as_deref() == Some("arc0"))
        .expect("arc0 is a polar point");
    let ctr = scene.points.iter().find(|p| p.name == "arc_ctr").expect("arc_ctr");
    let arc0 = scene.points.iter().find(|p| p.name == "arc0").expect("arc0");
    assert_eq!(ray.p0, ctr.at);
    assert_eq!(ray.p1, arc0.at);
    assert_eq!(ray.name, "arc0");

    // Ordinary lines are not rays.
    let a = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert!(a.lines.iter().all(|l| l.of.is_none()));
}

#[test]
fn arcs_report_their_ellipse() {
    let model = model("a22x-mono.mg");
    let scene = glyph_scene(&model, "Regular", "a").expect("glyph a");
    let stem = scene.paths.iter().find(|p| p.name.as_deref() == Some("stem")).expect("stem");
    let arc = stem.segments[1].arc.as_ref().expect("the first arc");
    let ctr = scene.points.iter().find(|p| p.name == "arc_ctr").expect("arc_ctr");
    assert_eq!(arc.center, ctr.at);
    // arc_radius = 0.533 * w, w = 500.
    assert!((arc.rx - 266.5).abs() < 1e-6, "{}", arc.rx);
    assert!((arc.ry - 266.5).abs() < 1e-6, "{}", arc.ry);
    assert!(arc.rx_expr.is_none(), "centre mode solves the radii");
    assert!(stem.segments[2].arc.is_none(), "a line has no ellipse");

    let ray = scene.lines.iter().find(|l| l.of.as_deref() == Some("arc0")).expect("ray");
    assert_eq!(ray.radius_expr.as_deref(), Some("arc_radius"));
}

#[test]
fn path_anchor_lies_on_the_skeleton() {
    let model = model("a22x-mono.mg");
    let scene = glyph_scene(&model, "Regular", "a").expect("glyph a");
    // `stem` starts with an arc about `arc_ctr`: the anchor is on that
    // circle, not on the chord between its ends.
    let stem = scene.paths.iter().find(|p| p.name.as_deref() == Some("stem")).expect("stem");
    let arc = stem.segments[1].arc.as_ref().expect("arc");
    let anchor = stem.anchor.expect("anchor");
    let r = (anchor[0] - arc.center[0]).hypot(anchor[1] - arc.center[1]);
    assert!((r - arc.rx).abs() < 0.5, "{r} vs {}", arc.rx);

    // A straight first segment: the anchor is its midpoint.
    let a = glyph_scene(&model, "Regular", "A").expect("glyph A");
    let stem = &a.paths[0];
    assert_eq!(stem.anchor, Some([125.0, 500.0]));
}

#[test]
fn measurements_report_their_ends() {
    let source = "font (name: \"T\", em: 1000)\nmetric baseline (y: 0)\nmetric xHeight (y: 500)\nmetric capHeight (y: 700)\nmetric ascender (y: 800)\nmetric descender (y: -200)\ninstance Regular ()\nglyph A (advance: 500) {\n    let a = (0, 0);\n    let b = (30, 40);\n    let d0 = length(b - a);\n    let half = 250;\n}\n";
    let model = analyze(source, 0).1.expect("parses");
    let scene = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert_eq!(scene.measures.len(), 1, "only `length(b - a)` is a measurement");
    let d0 = &scene.measures[0];
    assert_eq!(d0.name, "d0");
    assert_eq!((d0.a, d0.b, d0.value), ([0.0, 0.0], [30.0, 40.0], 50.0));
}
