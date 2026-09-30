use mg_web::{analyze, font_data, glyph_scene};

fn model(name: &str) -> mg_web::Model {
    let source = std::fs::read_to_string(format!(
        "{}/../../samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("sample exists");
    analyze(&source, 0).1.expect("sample parses")
}

#[test]
fn font_data_for_the_sample() {
    let model = model("metaglyph-sans.mg");
    let data = font_data(&model, "Regular").expect("instance exists");
    assert_eq!(data.em, 1000.0);

    let names: Vec<_> = data.metrics.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        ["baseline", "xHeight", "capHeight", "ascender", "descender"]
    );
    let cap = &data.metrics[2];
    assert_eq!(cap.expr, "700");
    assert_eq!(cap.y, Some(700.0));

    let lets: Vec<_> = data
        .lets
        .iter()
        .map(|l| (l.name.as_str(), l.expr.as_str(), l.value.as_str()))
        .collect();
    assert_eq!(
        lets[..3],
        [
            ("cell", "600", "600"),
            ("side", "60", "60"),
            ("x0", "side", "60")
        ]
    );

    let a = data.glyphs.iter().find(|g| g.name == "A").expect("glyph A");
    assert_eq!(a.codepoints, [0x41]);
    assert_eq!(a.fields.advance.as_deref(), Some("cell"));
    assert_eq!(a.advance, Some(600.0));
    assert!(a.ink.is_some());
    assert!(a.outline.starts_with('M'));
    assert_eq!(a.errors, 0);
}

#[test]
fn every_glyph_advances_one_cell() {
    let model = model("metaglyph-sans.mg");
    for instance in ["Light", "Regular", "Bold"] {
        let data = font_data(&model, instance).expect("instance exists");
        assert_eq!(data.glyphs.len(), 36);
        for g in &data.glyphs {
            assert_eq!(g.advance, Some(600.0), "{instance} {}", g.name);
            assert_eq!(g.errors, 0, "{instance} {}", g.name);
        }
    }
}

#[test]
fn unknown_instance_is_none() {
    let model = model("metaglyph-sans.mg");
    assert!(font_data(&model, "Nope").is_none());
    assert!(glyph_scene(&model, "Nope", "A").is_none());
    assert!(glyph_scene(&model, "Regular", "Nope").is_none());
}

#[test]
fn glyph_a_scene() {
    let model = model("metaglyph-sans.mg");
    let scene = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert!(!scene.outline.is_empty());

    let names: Vec<_> = scene.paths.iter().map(|p| p.name.as_deref()).collect();
    assert_eq!(names, [Some("legL"), Some("legR"), Some("bar")]);
    let bar = &scene.paths[2];
    assert_eq!(bar.stroke.as_deref(), Some("stem"));
    assert_eq!(bar.joins, "miter");
    // The legs are filled bands: none of their corners is a bare name.
    assert!(scene.paths[0].segments.iter().all(|s| s.to_ref.is_none()));

    let point = |name: &str| scene.points.iter().find(|p| p.name == name).expect(name);
    assert_eq!(point("footL").role, "construction");
    assert_eq!(point("footL").expr, "(x0 + dw / 2, 0)");

    let bar_y = scene
        .lines
        .iter()
        .find(|l| l.name == "bar_y")
        .expect("bar_y");
    assert_eq!(bar_y.expr, "hline(ym * 0.62)");
    assert!((bar_y.p0[1] - 336.0 * 0.62).abs() < 1e-9);

    // Spans point at the declaration text, in UTF-16 units.
    let units: Vec<u16> = model.source.encode_utf16().collect();
    let [from, to] = point("footL").span;
    let text = String::from_utf16(&units[from..to]).unwrap();
    assert_eq!(text, "let footL = (x0 + dw / 2, 0);");
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
    assert!(
        found,
        "some glyph in metaglyph-sans has a cube with controls"
    );
}

#[test]
fn polar_points_get_a_ray_from_their_origin() {
    let model = model("metaglyph-sans.mg");
    let scene = glyph_scene(&model, "Regular", "zero").expect("glyph zero");
    let ray = scene
        .lines
        .iter()
        .find(|l| l.of.as_deref() == Some("slash0"))
        .expect("slash0 is a polar point");
    let ctr = scene.points.iter().find(|p| p.name == "cBL").expect("cBL");
    let slash0 = scene
        .points
        .iter()
        .find(|p| p.name == "slash0")
        .expect("slash0");
    assert_eq!(ray.p0, ctr.at);
    assert_eq!(ray.p1, slash0.at);
    assert_eq!(ray.name, "slash0");
    assert_eq!(ray.radius_expr.as_deref(), Some("rad"));
    assert_eq!(slash0.role, "skeleton");
    assert_eq!(slash0.callee.as_deref(), Some("polar"));

    // Ordinary lines are not rays.
    let a = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert!(a.lines.iter().all(|l| l.of.is_none()));
}

#[test]
fn arcs_report_their_ellipse() {
    let model = model("metaglyph-sans.mg");
    let scene = glyph_scene(&model, "Regular", "zero").expect("glyph zero");
    let bowl = scene
        .paths
        .iter()
        .find(|p| p.name.as_deref() == Some("bowl"))
        .expect("bowl");
    let arc = bowl.segments[2].arc.as_ref().expect("the first corner");
    let ctr = scene.points.iter().find(|p| p.name == "cBL").expect("cBL");
    assert_eq!(arc.center, ctr.at);
    // rad = stem / 2 + corner = 45 + 50.
    assert!((arc.rx - 95.0).abs() < 1e-6, "{}", arc.rx);
    assert!((arc.ry - 95.0).abs() < 1e-6, "{}", arc.ry);
    assert!(arc.rx_expr.is_none(), "centre mode solves the radii");
    assert!(bowl.segments[1].arc.is_none(), "a line has no ellipse");
}

#[test]
fn path_anchor_lies_on_the_skeleton() {
    // A path starting with an arc: the anchor is on its circle, not on
    // the chord between its ends.
    let source = "font (name: \"T\", em: 1000)\nmetric baseline (y: 0)\nmetric xHeight (y: 500)\nmetric capHeight (y: 700)\nmetric ascender (y: 800)\nmetric descender (y: -200)\ninstance Regular ()\nglyph o (advance: 500) {\n    let ctr = (250, 300);\n    path bowl (stroke: 50) {\n        start (at: (50, 300))\n        arc   (center: ctr, to: (450, 300), sweep: \"cw\")\n    }\n}\n";
    let arc_model = analyze(source, 0).1.expect("parses");
    let scene = glyph_scene(&arc_model, "Regular", "o").expect("glyph o");
    let bowl = &scene.paths[0];
    let anchor = bowl.anchor.expect("anchor");
    let r = (anchor[0] - 250.0).hypot(anchor[1] - 300.0);
    assert!((r - 200.0).abs() < 0.5, "{r}");

    // A straight first segment: the anchor is its midpoint, here from
    // (zl, yt - rad) to (zl, yb + rad).
    let model = model("metaglyph-sans.mg");
    let zero = glyph_scene(&model, "Regular", "zero").expect("glyph zero");
    let bowl = zero
        .paths
        .iter()
        .find(|p| p.name.as_deref() == Some("bowl"))
        .expect("bowl");
    assert_eq!(bowl.anchor, Some([135.0, 350.0]));
}

#[test]
fn measurements_report_their_ends() {
    let source = "font (name: \"T\", em: 1000)\nmetric baseline (y: 0)\nmetric xHeight (y: 500)\nmetric capHeight (y: 700)\nmetric ascender (y: 800)\nmetric descender (y: -200)\ninstance Regular ()\nglyph A (advance: 500) {\n    let a = (0, 0);\n    let b = (30, 40);\n    let d0 = length(b - a);\n    let half = 250;\n}\n";
    let model = analyze(source, 0).1.expect("parses");
    let scene = glyph_scene(&model, "Regular", "A").expect("glyph A");
    assert_eq!(
        scene.measures.len(),
        1,
        "only `length(b - a)` is a measurement"
    );
    let d0 = &scene.measures[0];
    assert_eq!(d0.name, "d0");
    assert_eq!((d0.a, d0.b, d0.value), ([0.0, 0.0], [30.0, 40.0], 50.0));
}
