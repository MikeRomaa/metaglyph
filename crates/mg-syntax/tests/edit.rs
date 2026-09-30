//! The edit primitives (plan 5, §1.2): each case checks the exact result,
//! and `check` enforces the two invariants on every one — bytes outside
//! the edits are unchanged (by construction of `apply`, asserted here
//! directly) and the result has no syntax errors.

use mg_syntax::ast::{self, AstNode};
use mg_syntax::edit::{self, TextEdit};
use mg_syntax::index::{Def, Index};
use mg_syntax::{SyntaxKind, SyntaxNode, parse};

fn root(src: &str) -> SyntaxNode {
    let parsed = parse(src);
    assert!(parsed.diagnostics.is_empty(), "input parses: {:?}", parsed.diagnostics);
    parsed.syntax()
}

/// Applies `edits`, checks the invariants, returns the result.
fn check(src: &str, edits: &[TextEdit]) -> String {
    let out = edit::apply(src, edits);
    // Bytes before the first edit and after the last are untouched.
    let first = edits.iter().map(|e| e.range.start).min().unwrap_or(0);
    let last = edits.iter().map(|e| e.range.end).max().unwrap_or(src.len());
    assert_eq!(&out[..first], &src[..first]);
    assert!(out.ends_with(&src[last..]));
    let errors = parse(&out).diagnostics;
    assert!(errors.is_empty(), "result parses:\n{out}\n{errors:?}");
    out
}

fn find(root: &SyntaxNode, kind: SyntaxKind, name: &str) -> SyntaxNode {
    root.descendants()
        .filter(|n| n.kind() == kind)
        .find(|n| {
            n.children_with_tokens()
                .filter_map(|e| e.into_token())
                .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == name)
        })
        .unwrap_or_else(|| panic!("no {kind:?} {name}"))
}

fn let_value(root: &SyntaxNode, name: &str) -> ast::Expr {
    ast::LetStmt::cast(find(root, SyntaxKind::LET_STMT, name))
        .unwrap()
        .value()
        .unwrap()
}

fn field_value(decl: &SyntaxNode, name: &str) -> ast::Expr {
    edit::find_field(decl, name).unwrap().value().unwrap()
}

fn first_number(expr: &ast::Expr) -> mg_syntax::SyntaxToken {
    expr.syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::NUMBER)
        .unwrap()
}

// ── replace_literal ────────────────────────────────────────────────────

#[test]
fn replace_literal_keeps_decimals() {
    let src = "let a = (0.500 * w, h);\n";
    let root = root(src);
    let token = first_number(&let_value(&root, "a"));
    let out = check(src, &edit::replace_literal(&token, 0.5614));
    assert_eq!(out, "let a = (0.561 * w, h);\n");
}

#[test]
fn replace_literal_keeps_the_unit() {
    let src = "let a = polar(p, 10, 37deg);\n";
    let root = root(src);
    let token = let_value(&root, "a")
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::NUMBER_ANGLE)
        .unwrap();
    let out = check(src, &edit::replace_literal(&token, 41.26));
    assert_eq!(out, "let a = polar(p, 10, 41deg);\n");
}

#[test]
fn replace_literal_flips_sign_through_the_unary_minus() {
    let src = "let a = -15;\nlet b = 15;\n";
    let root = root(src);
    let a = first_number(&let_value(&root, "a"));
    assert_eq!(check(src, &edit::replace_literal(&a, -20.0)), "let a = -20;\nlet b = 15;\n");
    assert_eq!(check(src, &edit::replace_literal(&a, 5.0)), "let a = 5;\nlet b = 15;\n");
    let b = first_number(&let_value(&root, "b"));
    assert_eq!(check(src, &edit::replace_literal(&b, -3.0)), "let a = -15;\nlet b = -3;\n");
    // Rounds to zero: no sign.
    assert_eq!(check(src, &edit::replace_literal(&b, -0.2)), "let a = -15;\nlet b = 0;\n");
}

// ── add_constant ───────────────────────────────────────────────────────

#[test]
fn add_constant_rewrites_a_bare_literal() {
    let src = "kern (left: A, right: V, by: -15)\n";
    let root = root(src);
    let kern = root.descendants().find(|n| n.kind() == SyntaxKind::KERN).unwrap();
    let by = field_value(&kern, "by");
    assert_eq!(
        check(src, &edit::add_constant(&by, -5.0, 1000.0)),
        "kern (left: A, right: V, by: -20)\n"
    );
    assert_eq!(
        check(src, &edit::add_constant(&by, 25.0, 1000.0)),
        "kern (left: A, right: V, by: 10)\n"
    );
}

#[test]
fn add_constant_converts_into_em_literals() {
    let src = "kern (left: A, right: V, by: -0.015em)\n";
    let root = root(src);
    let kern = root.descendants().find(|n| n.kind() == SyntaxKind::KERN).unwrap();
    let by = field_value(&kern, "by");
    assert_eq!(
        check(src, &edit::add_constant(&by, -5.0, 1000.0)),
        "kern (left: A, right: V, by: -0.020em)\n"
    );
}

#[test]
fn add_constant_appends_then_updates_then_drops() {
    let src = "glyph A (codepoint: 'A', advance: s) {\n}\n";
    let root = root(src);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    let once = check(src, &edit::add_constant(&field_value(&glyph, "advance"), 12.4, 1000.0));
    assert_eq!(once, "glyph A (codepoint: 'A', advance: s + 12) {\n}\n");

    let root = self::root(&once);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    let twice = check(&once, &edit::add_constant(&field_value(&glyph, "advance"), -20.0, 1000.0));
    assert_eq!(twice, "glyph A (codepoint: 'A', advance: s - 8) {\n}\n");

    let root = self::root(&twice);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    let back = check(&twice, &edit::add_constant(&field_value(&glyph, "advance"), 8.0, 1000.0));
    assert_eq!(back, src);
}

#[test]
fn add_constant_parenthesizes_a_loose_operator() {
    let src = "let a = b or c;\n";
    let root = root(src);
    let out = check(src, &edit::add_constant(&let_value(&root, "a"), 3.0, 1000.0));
    assert_eq!(out, "let a = (b or c) + 3;\n");
}

#[test]
fn add_constant_of_nothing_is_no_edit() {
    let src = "let a = b * 2;\n";
    let root = root(src);
    assert!(edit::add_constant(&let_value(&root, "a"), 0.3, 1000.0).is_empty());
}

// ── convert_unit ───────────────────────────────────────────────────────

fn kern_by(src: &str) -> ast::Expr {
    let kern = root(src).descendants().find(|n| n.kind() == SyntaxKind::KERN).unwrap();
    field_value(&kern, "by")
}

#[test]
fn convert_unit_round_trips_a_bare_literal() {
    let raw = "kern (left: A, right: V, by: -15)\n";
    let em = "kern (left: A, right: V, by: -0.015em)\n";
    assert_eq!(check(raw, &edit::convert_unit(&kern_by(raw), true, 1000.0).unwrap()), em);
    assert_eq!(check(em, &edit::convert_unit(&kern_by(em), false, 1000.0).unwrap()), raw);
    // Already in the unit.
    assert!(edit::convert_unit(&kern_by(raw), false, 1000.0).is_none());
}

#[test]
fn convert_unit_takes_the_trailing_constant_or_a_factor() {
    let src = "kern (left: A, right: V, by: k - 20)\n";
    assert_eq!(
        check(src, &edit::convert_unit(&kern_by(src), true, 1000.0).unwrap()),
        "kern (left: A, right: V, by: k - 0.020em)\n"
    );
    let src = "kern (left: A, right: V, by: -0.05em * kernStrength)\n";
    assert_eq!(
        check(src, &edit::convert_unit(&kern_by(src), false, 1000.0).unwrap()),
        "kern (left: A, right: V, by: -50 * kernStrength)\n"
    );
    let src = "kern (left: A, right: V, by: k / 2)\n";
    assert!(edit::convert_unit(&kern_by(src), true, 1000.0).is_none());
}

// ── list_insert / list_remove ──────────────────────────────────────────

fn group_list(src: &str) -> ast::ListExpr {
    let group = root(src).descendants().find(|n| n.kind() == SyntaxKind::GROUP).unwrap();
    match field_value(&group, "glyphs") {
        ast::Expr::List(list) => list,
        other => panic!("{other:?}"),
    }
}

#[test]
fn list_insert_is_comma_aware() {
    let cases = [
        ("group g (glyphs: [ o, c ])\n", "group g (glyphs: [ o, c, e ])\n"),
        ("group g (glyphs: [o, c,])\n", "group g (glyphs: [o, c, e])\n"),
        ("group g (glyphs: [ ])\n", "group g (glyphs: [ e ])\n"),
        ("group g (glyphs: [])\n", "group g (glyphs: [e])\n"),
        (
            "group g (glyphs: [\n    o,\n    c,\n])\n",
            "group g (glyphs: [\n    o,\n    c,\n    e,\n])\n",
        ),
    ];
    for (src, want) in cases {
        let list = group_list(src);
        assert_eq!(check(src, &[edit::list_insert(src, &list, "e")]), want, "{src}");
    }
}

#[test]
fn list_remove_takes_one_comma() {
    let src = "group g (glyphs: [ o, c, e ])\n";
    let list = group_list(src);
    let remove = |i| check(src, &edit::list_remove(src, &list, i).unwrap());
    assert_eq!(remove(0), "group g (glyphs: [ c, e ])\n");
    assert_eq!(remove(1), "group g (glyphs: [ o, e ])\n");
    assert_eq!(remove(2), "group g (glyphs: [ o, c ])\n");
    assert!(edit::list_remove(src, &list, 3).is_none());

    let src = "group g (glyphs: [\n    o,\n    c,\n])\n";
    let list = group_list(src);
    assert_eq!(
        check(src, &edit::list_remove(src, &list, 0).unwrap()),
        "group g (glyphs: [\n    c,\n])\n"
    );
}

// ── set_field / remove_field ───────────────────────────────────────────

#[test]
fn set_field_replaces_an_existing_value() {
    let src = "glyph A (codepoint: 'A', advance: s) {\n}\n";
    let root = root(src);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    let out = check(src, &edit::set_field(src, &glyph, "advance", "600"));
    assert_eq!(out, "glyph A (codepoint: 'A', advance: 600) {\n}\n");
}

#[test]
fn set_field_appends_on_a_single_line() {
    let src = "glyph A (codepoint: 'A', advance: s) {\n}\n";
    let root = root(src);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    let out = check(src, &edit::set_field(src, &glyph, "lsb", "15"));
    assert_eq!(out, "glyph A (codepoint: 'A', advance: s, lsb: 15) {\n}\n");
}

#[test]
fn set_field_aligns_on_a_multi_line_config() {
    let src = "glyph a (advance: s) {\n    path p (stroke: 50,\n            caps: \"round\") {\n        start (at: (0, 0))\n    }\n}\n";
    let root = root(src);
    let path = find(&root, SyntaxKind::PATH, "p");
    let out = check(src, &edit::set_field(src, &path, "joins", "\"bevel\""));
    assert_eq!(
        out,
        "glyph a (advance: s) {\n    path p (stroke: 50,\n            caps: \"round\",\n            joins: \"bevel\") {\n        start (at: (0, 0))\n    }\n}\n"
    );
}

#[test]
fn set_field_fills_an_empty_or_missing_config() {
    let src = "instance Regular ()\nglyph a (advance: 1) {\n    path p {\n        start (at: (0, 0))\n    }\n}\n";
    let root = root(src);
    let instance = find(&root, SyntaxKind::INSTANCE, "Regular");
    assert_eq!(
        check(src, &edit::set_field(src, &instance, "slant", "12deg")),
        src.replace("Regular ()", "Regular (slant: 12deg)")
    );
    let path = find(&root, SyntaxKind::PATH, "p");
    assert_eq!(
        check(src, &edit::set_field(src, &path, "stroke", "50")),
        src.replace("path p {", "path p (stroke: 50) {")
    );
}

#[test]
fn remove_field_takes_one_comma() {
    let src = "glyph A (codepoint: 'A', advance: s, lsb: 15) {\n}\n";
    let root = root(src);
    let glyph = find(&root, SyntaxKind::GLYPH, "A");
    assert_eq!(
        check(src, &edit::remove_field(src, &glyph, "advance").unwrap()),
        "glyph A (codepoint: 'A', lsb: 15) {\n}\n"
    );
    assert_eq!(
        check(src, &edit::remove_field(src, &glyph, "lsb").unwrap()),
        "glyph A (codepoint: 'A', advance: s) {\n}\n"
    );
    assert!(edit::remove_field(src, &glyph, "rsb").is_none());
}

#[test]
fn remove_field_on_its_own_line_takes_the_line() {
    let src = "font (name: \"X\",\n      em: 1000,\n      version: \"1.0\")\n";
    let root = root(src);
    let font = root.descendants().find(|n| n.kind() == SyntaxKind::FONT).unwrap();
    assert_eq!(
        check(src, &edit::remove_field(src, &font, "em").unwrap()),
        "font (name: \"X\",\n      version: \"1.0\")\n"
    );
    // The last field shares its line with `)`: it takes the comma before.
    assert_eq!(
        check(src, &edit::remove_field(src, &font, "version").unwrap()),
        "font (name: \"X\",\n      em: 1000)\n"
    );
}

// ── declarations ───────────────────────────────────────────────────────

const GLYPH: &str = "glyph A (advance: 1) {\n    let a = (0, 0);\n    let b = (1, 1);\n\n    path p (stroke: 50) {\n        start (at: a)\n        line  (to: b)\n    }\n}\n";

fn body(root: &SyntaxNode, glyph: &str) -> ast::Body {
    ast::Glyph::cast(find(root, SyntaxKind::GLYPH, glyph))
        .unwrap()
        .body()
        .unwrap()
}

#[test]
fn insert_let_goes_after_the_last_let() {
    let root = root(GLYPH);
    let out = check(GLYPH, &[edit::insert_let(GLYPH, &body(&root, "A"), "let p0 = (5, 5);")]);
    assert_eq!(out, GLYPH.replace("(1, 1);\n", "(1, 1);\n    let p0 = (5, 5);\n"));
}

#[test]
fn insert_let_without_lets_leads_with_a_blank_line() {
    let src = "glyph A (advance: 1) {\n    path p (stroke: 50) {\n        start (at: (0, 0))\n    }\n}\n";
    let root = root(src);
    let out = check(src, &[edit::insert_let(src, &body(&root, "A"), "let p0 = (5, 5);")]);
    assert_eq!(out, src.replace("{\n    path", "{\n    let p0 = (5, 5);\n\n    path"));
}

#[test]
fn insert_into_an_empty_body() {
    for (src, expected) in [
        ("glyph A (rsb: 0) {\n}\n", "glyph A (rsb: 0) {\n    let p0 = (5, 5);\n}\n"),
        ("glyph A (rsb: 0) {}\n", "glyph A (rsb: 0) {\n    let p0 = (5, 5);\n}\n"),
    ] {
        let root = root(src);
        let out = check(src, &[edit::insert_let(src, &body(&root, "A"), "let p0 = (5, 5);")]);
        assert_eq!(out, expected);
    }
}

#[test]
fn insert_decl_after_a_segment_copies_its_indent() {
    let root = root(GLYPH);
    let path = ast::Path::cast(find(&root, SyntaxKind::PATH, "p")).unwrap();
    let line = path.body().unwrap().items().last().unwrap();
    let out = check(GLYPH, &[edit::insert_decl(GLYPH, &path.body().unwrap(), Some(&line), "line  (to: a)")]);
    assert_eq!(out, GLYPH.replace("(to: b)\n", "(to: b)\n        line  (to: a)\n"));
}

#[test]
fn insert_top_level_groups_by_kind() {
    let src = "metric baseline (y: 0)\n\nglyph A (advance: 1) {\n}\n";
    let root = root(src);
    let file = ast::SourceFile::cast(root.clone()).unwrap();
    assert_eq!(
        check(src, &[edit::insert_top_level(src, &file, SyntaxKind::METRIC, "metric xHeight (y: 500)")]),
        "metric baseline (y: 0)\nmetric xHeight (y: 500)\n\nglyph A (advance: 1) {\n}\n"
    );
    assert_eq!(
        check(src, &[edit::insert_top_level(src, &file, SyntaxKind::GLYPH, "glyph B (rsb: 0) {\n}")]),
        "metric baseline (y: 0)\n\nglyph A (advance: 1) {\n}\n\nglyph B (rsb: 0) {\n}\n"
    );
    assert_eq!(
        check(src, &[edit::insert_top_level(src, &file, SyntaxKind::KERN, "kern (left: A, right: A, by: -15)")]),
        "metric baseline (y: 0)\n\nglyph A (advance: 1) {\n}\n\nkern (left: A, right: A, by: -15)\n"
    );
}

#[test]
fn remove_decl_takes_its_line() {
    let root = root(GLYPH);
    let b = find(&root, SyntaxKind::LET_STMT, "b");
    assert_eq!(check(GLYPH, &[edit::remove_decl(GLYPH, &b)]), GLYPH.replace("    let b = (1, 1);\n", ""));
}

#[test]
fn remove_decl_collapses_blank_lines() {
    let src = "glyph A (advance: 1) {\n    let a = (0, 0);\n\n    path p (stroke: 50) {\n        start (at: a)\n    }\n\n    path q (stroke: 50) {\n        start (at: a)\n    }\n}\n";
    let root = root(src);
    // The only `let`: the blank line after it would open the body.
    let a = find(&root, SyntaxKind::LET_STMT, "a");
    assert_eq!(
        check(src, &[edit::remove_decl(src, &a)]),
        src.replace("    let a = (0, 0);\n\n", "")
    );
    // A path between blank lines leaves one.
    let p = find(&root, SyntaxKind::PATH, "p");
    assert_eq!(
        check(src, &[edit::remove_decl(src, &p)]),
        src.replace("    path p (stroke: 50) {\n        start (at: a)\n    }\n\n", "")
    );
}

// ── names ──────────────────────────────────────────────────────────────

#[test]
fn rename_updates_every_reference_in_scope() {
    let src = "let a = 1;\nglyph A (advance: 1) {\n    let a = (0, 0);\n    path p (stroke: 50) {\n        start (at: a)\n    }\n}\nglyph B (advance: a) {\n}\n";
    let root = root(src);
    let index = Index::new(&ast::SourceFile::cast(root.clone()).unwrap());
    let local = Def::GlyphLocal { glyph: 0, name: "a".to_string() };
    let out = check(src, &edit::rename(&root, &index, &local, "origin"));
    assert_eq!(
        out,
        src.replace("let a = (0, 0)", "let origin = (0, 0)").replace("(at: a)", "(at: origin)")
    );
    let top = Def::TopLevel("a".to_string());
    let out = check(src, &edit::rename(&root, &index, &top, "unit"));
    assert_eq!(out, src.replace("let a = 1", "let unit = 1").replace("(advance: a)", "(advance: unit)"));
}

#[test]
fn invalid_names() {
    assert_eq!(edit::invalid_name("stem1"), None);
    assert_eq!(edit::invalid_name("_x"), None);
    assert_eq!(edit::invalid_name("1x"), Some("not an identifier"));
    assert_eq!(edit::invalid_name("a-b"), Some("not an identifier"));
    assert_eq!(edit::invalid_name(""), Some("not an identifier"));
    assert_eq!(edit::invalid_name("glyph"), Some("a reserved word"));
}

// ── on the samples ─────────────────────────────────────────────────────

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../samples/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn removing_every_let_in_a_sample_keeps_it_parsing() {
    for name in ["a22x-mono.mg", "metaglyph-sans.mg"] {
        let src = sample(name);
        let root = root(&src);
        for node in root.descendants().filter(|n| n.kind() == SyntaxKind::LET_STMT) {
            check(&src, &[edit::remove_decl(&src, &node)]);
        }
    }
}

#[test]
fn replacing_every_literal_in_a_sample_keeps_it_parsing() {
    for name in ["a22x-mono.mg", "metaglyph-sans.mg"] {
        let src = sample(name);
        let root = root(&src);
        for token in root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| edit::literal_value(t).is_some())
        {
            for value in [-12.345, 0.0, 987.6] {
                check(&src, &edit::replace_literal(&token, value));
            }
        }
    }
}
