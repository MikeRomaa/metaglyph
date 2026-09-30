//! Edit ops (plan 5, §1.1, §1.4): each reads the current text's CST and
//! returns the text changes for the editor to apply as one CodeMirror
//! transaction. The editor never builds `.mg` text itself.
//!
//! Ops address existing declarations by their source span (UTF-16, as the
//! views report it) in a given document version, and glyphs and paths by
//! name; a stale version or a text with syntax errors is refused.
//!
//! An op may take several steps (insert a `let`, then the segment that
//! uses it): each step is computed on the text the previous one produced,
//! so the primitives never see overlapping edits. The editor composes the
//! steps into one change.

use std::collections::HashSet;

use mg_syntax::ast::{self, AstNode};
use mg_syntax::edit::{self, TextEdit};
use mg_syntax::index::{Def, Index};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use serde::{Deserialize, Serialize};

use crate::offsets::Utf16Index;

type Pt = [f64; 2];

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    /// Rename the declaration at `span`, and every reference to it.
    Rename { span: [usize; 2], name: String },
    /// Delete the declaration at `span` (plan 5: dangling references
    /// become diagnostics, as intended). A segment takes the `let`s only
    /// it used with it.
    Delete { span: [usize; 2] },
    /// `let pN = (x, y);`
    #[serde(rename_all = "camelCase")]
    AddPoint { glyph: String, at: Pt },
    /// `let lN = lineThrough(a, b);`, `hline(y)` or `vline(x)`.
    #[serde(rename_all = "camelCase")]
    AddLine { glyph: String, line: LineSpec },
    /// `let dN = length(b - a);`
    #[serde(rename_all = "camelCase")]
    AddMeasure { glyph: String, a: String, b: String },
    /// A new path from a new point: `let pN = (x, y);` and
    /// `path pathN (<config copied from copy_from>) { start (at: pN) }`.
    #[serde(rename_all = "camelCase")]
    PathStart {
        glyph: String,
        at: Pt,
        copy_from: Option<String>,
    },
    /// A new point and a segment to it at the end of `path` (before its
    /// `close`): `line`, or `cube` when control points are given.
    #[serde(rename_all = "camelCase")]
    PathAppend {
        glyph: String,
        path: String,
        at: Pt,
        c1: Option<Pt>,
        c2: Option<Pt>,
    },
    /// `close` at the end of `path`.
    #[serde(rename_all = "camelCase")]
    PathClose { glyph: String, path: String },
    /// Rewrite the segment at `span` as `kind`, with new control `let`s at
    /// `controls` (quad: `c`; cube: `c1`, `c2`; arc: its centre). Control
    /// `let`s only the old segment used are removed.
    #[serde(rename_all = "camelCase")]
    SetSegmentKind {
        span: [usize; 2],
        kind: String,
        controls: Vec<Pt>,
    },
    /// Set a config field of the declaration at `span`.
    #[serde(rename_all = "camelCase")]
    SetField {
        span: [usize; 2],
        name: String,
        value: FieldValue,
    },
    /// Remove a config field of the declaration at `span`.
    #[serde(rename_all = "camelCase")]
    RemoveField { span: [usize; 2], name: String },
    /// Fill on: `fill: true`, plus `close` if the path has none. Off:
    /// remove `fill` (the `close` stays).
    #[serde(rename_all = "camelCase")]
    SetFill { span: [usize; 2], on: bool },
    /// `component (glyph: target, offset: (dx, dy))`.
    #[serde(rename_all = "camelCase")]
    AddComponent {
        glyph: String,
        target: String,
        offset: Pt,
    },
    /// A relationship tool (plan 5, §1.4): rewrite the local point `let`
    /// at `span` as `relation`.
    #[serde(rename_all = "camelCase")]
    Relate { span: [usize; 2], relation: Relation },
    /// Add `delta` (raw units) to field `name` of the declaration at
    /// `span` (plan 5, §1.2 `add_constant`): a metric line drag, or a
    /// typed number. `em` converts into `em`/`%` literals.
    #[serde(rename_all = "camelCase")]
    AddConstant {
        span: [usize; 2],
        name: String,
        delta: f64,
        em: f64,
    },
    /// A spacing drag or typed bearing (plan 5, §1.6), given the glyph's
    /// current `lsb` and `rsb`: `edge` says what moves by `delta` (see
    /// [`Edge`]).
    #[serde(rename_all = "camelCase")]
    Spacing {
        glyph: String,
        edge: Edge,
        delta: f64,
        lsb: f64,
        rsb: f64,
        em: f64,
    },
    /// Set field `name` of the `font (…)` directive, or remove it when
    /// `value` is absent (plan 5, §1.6 "Font info form").
    #[serde(rename_all = "camelCase")]
    FontField {
        name: String,
        value: Option<FieldValue>,
    },
}

/// What an [`Op::Spacing`] moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    /// The origin guide: `lsb` += delta, `rsb` stays (the advance grows).
    Left,
    /// The advance guide: `rsb` += delta, `lsb` stays.
    Right,
    /// The ink, within the advance: `lsb` += delta, `rsb` -= delta.
    Ink,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Relation {
    /// `b`
    Coincident { b: String },
    /// `meet(l1, l2)`
    Meet { l1: LineRef, l2: LineRef },
    /// `project((x, y), line)`, from the point's current position.
    Project { at: Pt, line: LineRef },
    /// `mediate(a, b, t)`.
    Fraction { a: String, b: String, t: f64 },
    /// `polar(q, len, θdeg)`.
    Polar { q: String, len: f64, angle: f64 },
    /// `mirror(q, axis)`.
    Mirror { q: String, axis: LineRef },
}

/// A line: a named `let`, or `lineThrough(a, b)` of two named points (a
/// straight segment's ends).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum LineRef {
    Named { name: String },
    Through { through: [String; 2] },
}

impl LineRef {
    fn text(&self) -> String {
        match self {
            LineRef::Named { name } => name.clone(),
            LineRef::Through { through: [a, b] } => format!("lineThrough({a}, {b})"),
        }
    }

    fn names(&self) -> Vec<&str> {
        match self {
            LineRef::Named { name } => vec![name],
            LineRef::Through { through } => through.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LineSpec {
    Through { a: String, b: String },
    Hline { y: f64 },
    Vline { x: f64 },
}

/// A field value. `Expr` is text the user typed into the inspector, which
/// is spliced as-is (plan 5, §2.4).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum FieldValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Expr(String),
}

/// One replacement, in UTF-16 offsets of the text its step applies to.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

/// A declaration an op created, for the editor to select and offer to
/// rename (plan 5, §1.3).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    /// `point`, `line`, `path`, `let` (a measurement) or `component`.
    pub kind: &'static str,
    pub name: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum EditResult {
    /// `steps[i]` applies to the text after `steps[..i]`.
    Ok {
        version: u32,
        steps: Vec<Vec<Change>>,
        created: Option<Created>,
        /// What the op did, for a live callout (`ADD lsb: 15`,
        /// `advance += 40`).
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// The text changed since `version`; ask again.
    Stale,
    /// The text has syntax errors; edits wait until it parses (plan 5,
    /// §1.1).
    ReadOnly,
    /// The op can't apply, with a message for the user.
    Invalid { message: String },
}

type OpResult<T> = Result<T, String>;

/// The text as an op builds it up, one step at a time.
struct Session {
    src: String,
    steps: Vec<Vec<Change>>,
    summary: Option<String>,
}

impl Session {
    /// Runs `f` on the current text and applies its edits as a step.
    fn step(
        &mut self,
        f: impl FnOnce(&str, &SyntaxNode) -> OpResult<Vec<TextEdit>>,
    ) -> OpResult<()> {
        let root = mg_syntax::parse(&self.src).syntax();
        let edits = merge_deletions(f(&self.src, &root)?);
        if edits.is_empty() {
            return Ok(());
        }
        let offsets = Utf16Index::new(&self.src);
        self.steps.push(
            edits
                .iter()
                .map(|e| Change {
                    from: offsets.convert(e.range.start),
                    to: offsets.convert(e.range.end),
                    insert: e.text.clone(),
                })
                .collect(),
        );
        self.src = edit::apply(&self.src, &edits);
        Ok(())
    }
}

/// Deletions that overlap (two removed lines sharing a collapsed blank
/// line) merged into one.
fn merge_deletions(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    let mut out: Vec<TextEdit> = Vec::new();
    for e in edits {
        if let Some(last) = out.last_mut()
            && last.text.is_empty()
            && e.text.is_empty()
            && e.range.start < last.range.end
        {
            last.range.end = last.range.end.max(e.range.end);
            continue;
        }
        out.push(e);
    }
    out
}

/// Runs `op` against `source` (document `version`).
pub fn run(source: &str, version: u32, op: &Op) -> EditResult {
    let parsed = mg_syntax::parse(source);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == mg_diag::Severity::Error)
    {
        return EditResult::ReadOnly;
    }
    let offsets = Utf16Index::new(source);
    let span = |s: &[usize; 2]| offsets.to_byte(s[0])..offsets.to_byte(s[1]);

    let mut session = Session {
        src: source.to_string(),
        steps: Vec::new(),
        summary: None,
    };
    match apply(&mut session, op, &span) {
        Ok(created) => EditResult::Ok {
            version,
            steps: session.steps,
            created,
            summary: session.summary,
        },
        Err(message) => EditResult::Invalid { message },
    }
}

fn apply(
    s: &mut Session,
    op: &Op,
    span: &dyn Fn(&[usize; 2]) -> std::ops::Range<usize>,
) -> OpResult<Option<Created>> {
    const GONE: &str = "That declaration is no longer in the source.";
    match op {
        Op::Rename { span: at, name } => {
            let range = span(at);
            s.step(|_, root| {
                let node = decl_at(root, range).ok_or(GONE)?;
                rename(root, &node, name)
            })?;
            Ok(None)
        }
        Op::Delete { span: at } => {
            let range = span(at);
            s.step(|src, root| {
                let node = decl_at(root, range).ok_or(GONE)?;
                let mut edits = vec![edit::remove_decl(src, &node)];
                if is_segment(node.kind()) {
                    for decl in lets_only_used_by(root, &node) {
                        edits.push(edit::remove_decl(src, &decl));
                    }
                }
                Ok(edits)
            })?;
            Ok(None)
        }
        Op::AddPoint { glyph, at } => {
            let name = add_let(s, glyph, "p", |_| Ok(point_text(*at)))?;
            Ok(Some(Created {
                kind: "point",
                name,
            }))
        }
        Op::AddLine { glyph, line } => {
            let name = add_let(s, glyph, "l", |_| {
                Ok(match line {
                    LineSpec::Through { a, b } => format!("lineThrough({a}, {b})"),
                    LineSpec::Hline { y } => format!("hline({})", coord(*y)),
                    LineSpec::Vline { x } => format!("vline({})", coord(*x)),
                })
            })?;
            Ok(Some(Created { kind: "line", name }))
        }
        Op::AddMeasure { glyph, a, b } => {
            let name = add_let(s, glyph, "d", |_| Ok(format!("length({b} - {a})")))?;
            Ok(Some(Created { kind: "let", name }))
        }
        Op::PathStart {
            glyph,
            at,
            copy_from,
        } => {
            let point = add_let(s, glyph, "p", |_| Ok(point_text(*at)))?;
            let mut name = String::new();
            s.step(|src, root| {
                let g = glyph_named(root, glyph)?;
                name = fresh(root, &g, "path");
                let config = copy_from
                    .as_ref()
                    .and_then(|p| path_named(&g, p))
                    .map(|p| copied_config(&p))
                    .unwrap_or_default();
                let body = g.body().ok_or("This glyph has no body.")?;
                Ok(vec![insert_path(src, &body, &name, &config, &point)])
            })?;
            Ok(Some(Created { kind: "path", name }))
        }
        Op::PathAppend {
            glyph,
            path,
            at,
            c1,
            c2,
        } => {
            let point = add_let(s, glyph, "p", |_| Ok(point_text(*at)))?;
            // Smooth by reflection (spec §6.3): after a `cube`, `c1` is
            // omitted.
            let previous_cube = {
                let root = mg_syntax::parse(&s.src).syntax();
                let g = glyph_named(&root, glyph)?;
                let p = path_named(&g, path).ok_or("That path is no longer in the source.")?;
                last_segment(&p).is_some_and(|n| n.kind() == SyntaxKind::CUBE)
            };
            let segment = match c2 {
                Some(c2) => {
                    let c1_field = match (c1, previous_cube) {
                        (Some(c1), false) => {
                            let c1_name =
                                add_named_let(s, glyph, &format!("{point}_c1"), point_text(*c1))?;
                            format!("c1: {c1_name}, ")
                        }
                        _ => String::new(),
                    };
                    let c2_name = add_named_let(s, glyph, &format!("{point}_c2"), point_text(*c2))?;
                    format!("cube  ({c1_field}c2: {c2_name}, to: {point})")
                }
                None => format!("line  (to: {point})"),
            };
            s.step(|src, root| {
                let g = glyph_named(root, glyph)?;
                let p = path_named(&g, path).ok_or("That path is no longer in the source.")?;
                Ok(vec![append_segment(src, &p, &segment)?])
            })?;
            Ok(Some(Created {
                kind: "point",
                name: point,
            }))
        }
        Op::PathClose { glyph, path } => {
            s.step(|src, root| {
                let g = glyph_named(root, glyph)?;
                let p = path_named(&g, path).ok_or("That path is no longer in the source.")?;
                if has_close(&p) {
                    return Ok(Vec::new());
                }
                Ok(vec![append_segment(src, &p, "close")?])
            })?;
            Ok(None)
        }
        Op::SetSegmentKind {
            span: at,
            kind,
            controls,
        } => {
            set_segment_kind(s, span(at), kind, controls)?;
            Ok(None)
        }
        Op::SetField {
            span: at,
            name,
            value,
        } => {
            let range = span(at);
            let text = field_text(value)?;
            s.step(|src, root| {
                let node = decl_at(root, range).ok_or(GONE)?;
                Ok(edit::set_field(src, &node, name, &text))
            })?;
            check_parses(s, "That value doesn't parse.")?;
            Ok(None)
        }
        Op::RemoveField { span: at, name } => {
            let range = span(at);
            s.step(|src, root| {
                let node = decl_at(root, range).ok_or(GONE)?;
                Ok(edit::remove_field(src, &node, name).unwrap_or_default())
            })?;
            Ok(None)
        }
        Op::SetFill { span: at, on } => {
            let range = span(at);
            if *on {
                s.step(|src, root| {
                    let node = decl_at(root, range.clone()).ok_or(GONE)?;
                    Ok(edit::set_field(src, &node, "fill", "true"))
                })?;
                // Adding a field never moves the path's start.
                s.step(|src, root| {
                    let p = root
                        .descendants()
                        .filter_map(ast::Path::cast)
                        .find(|p| edit::node_range(p.syntax()).start == range.start)
                        .ok_or(GONE)?;
                    if has_close(&p) {
                        return Ok(Vec::new());
                    }
                    Ok(vec![append_segment(src, &p, "close")?])
                })?;
            } else {
                s.step(|src, root| {
                    let node = decl_at(root, range).ok_or(GONE)?;
                    Ok(edit::remove_field(src, &node, "fill").unwrap_or_default())
                })?;
            }
            Ok(None)
        }
        Op::AddComponent {
            glyph,
            target,
            offset,
        } => {
            s.step(|src, root| {
                let g = glyph_named(root, glyph)?;
                if glyph_named(root, target).is_err() {
                    return Err(format!("There is no glyph `{target}`."));
                }
                if target == glyph {
                    return Err("A glyph can't contain itself.".to_string());
                }
                let body = g.body().ok_or("This glyph has no body.")?;
                let last = body.items().last();
                Ok(vec![edit::insert_decl(
                    src,
                    &body,
                    last.as_ref(),
                    &format!("component (glyph: {target}, offset: {})", point_text(*offset)),
                )])
            })?;
            Ok(Some(Created {
                kind: "component",
                name: target.clone(),
            }))
        }
        Op::Relate { span: at, relation } => {
            relate(s, span(at), relation)?;
            Ok(None)
        }
        Op::AddConstant {
            span: at,
            name,
            delta,
            em,
        } => {
            let range = span(at);
            s.step(|_, root| {
                let node = decl_at(root, range).ok_or(GONE)?;
                let value = edit::find_field(&node, name)
                    .and_then(|f| f.value())
                    .ok_or_else(|| format!("There is no `{name}` field to change."))?;
                Ok(edit::add_constant(&value, *delta, *em))
            })?;
            s.summary = Some(format!("{name} {}", signed(*delta, "+= ", "-= ")));
            Ok(None)
        }
        Op::Spacing {
            glyph,
            edge,
            delta,
            lsb,
            rsb,
            em,
        } => {
            spacing(s, glyph, *edge, *delta, [*lsb, *rsb], *em)?;
            Ok(None)
        }
        Op::FontField { name, value } => {
            let text = value.as_ref().map(field_text).transpose()?;
            s.step(|src, root| {
                let font = root
                    .children()
                    .find(|n| n.kind() == SyntaxKind::FONT)
                    .ok_or("The source has no `font (…)` directive.")?;
                Ok(match &text {
                    Some(text) => edit::set_field(src, &font, name, text),
                    None => edit::remove_field(src, &font, name).unwrap_or_default(),
                })
            })?;
            check_parses(s, "That value doesn't parse.")?;
            Ok(None)
        }
    }
}

/// A field value as source text.
fn field_text(value: &FieldValue) -> OpResult<String> {
    Ok(match value {
        FieldValue::Str(v) => format!("{v:?}"),
        FieldValue::Num(v) => number(*v),
        FieldValue::Bool(v) => v.to_string(),
        FieldValue::Expr(v) => {
            let v = v.trim();
            if v.is_empty() {
                return Err("Type a value.".to_string());
            }
            v.to_string()
        }
    })
}

/// A spacing change (plan 5, §1.6): the bearings change by what `edge`
/// says, and the advance by their sum. Each declared field add-constants
/// its own change. When the declared fields can't express the change, one
/// more is declared at its new value:
///
/// - `advance` or `rsb` alone fix the ink where it was authored, so
///   moving it needs `lsb`;
/// - `lsb` alone makes both bearings equal, so changing them apart needs
///   `rsb`.
///
/// | Declared         | Left                   | Right                  | Ink                 |
/// |------------------|------------------------|------------------------|---------------------|
/// | `advance`        | `advance` += d, add `lsb` | `advance` += d      | add `lsb`           |
/// | `rsb`            | add `lsb`              | `rsb` += d             | `rsb` -= d, add `lsb` |
/// | `lsb`            | `lsb` += d, add `rsb`  | add `rsb`              | `lsb` += d, add `rsb` |
/// | `lsb`, `rsb`     | `lsb` += d             | `rsb` += d             | `lsb` += d, `rsb` -= d |
/// | `advance`, `lsb` | `advance`, `lsb` += d  | `advance` += d         | `lsb` += d          |
/// | `advance`, `rsb` | `advance` += d         | `advance`, `rsb` += d  | `rsb` -= d          |
fn spacing(
    s: &mut Session,
    glyph: &str,
    edge: Edge,
    delta: f64,
    [lsb, rsb]: [f64; 2],
    em: f64,
) -> OpResult<()> {
    let (d_lsb, d_rsb) = match edge {
        Edge::Left => (delta, 0.0),
        Edge::Right => (0.0, delta),
        Edge::Ink => (delta, -delta),
    };
    let change = |name: &str| match name {
        "lsb" => d_lsb,
        "rsb" => d_rsb,
        _ => d_lsb + d_rsb,
    };
    let mut what = Vec::new();
    let mut declare = None;
    s.step(|_, root| {
        let g = glyph_named(root, glyph)?;
        let has = |f| edit::find_field(g.syntax(), f).is_some();
        let (advance, left, right) = (has("advance"), has("lsb"), has("rsb"));
        declare = match (advance, left, right) {
            (false, false, false) => {
                return Err(format!("glyph {glyph} declares no spacing field."));
            }
            (_, false, _) if d_lsb.round() != 0.0 && !(advance && right) => {
                Some(("lsb", lsb + d_lsb))
            }
            (false, true, false) if (d_rsb - d_lsb).round() != 0.0 => Some(("rsb", rsb + d_rsb)),
            _ => None,
        };
        let mut edits = Vec::new();
        for (name, declared) in [("advance", advance), ("lsb", left), ("rsb", right)] {
            let d = change(name);
            if !declared || d.round() == 0.0 {
                continue;
            }
            let value = edit::find_field(g.syntax(), name)
                .and_then(|f| f.value())
                .ok_or_else(|| format!("glyph {glyph}'s `{name}` has no value."))?;
            edits.extend(edit::add_constant(&value, d, em));
            what.push(format!("{name} {}", signed(d, "+= ", "-= ")));
        }
        Ok(edits)
    })?;
    // A new field after the constants: appended where one may end.
    if let Some((name, value)) = declare {
        s.step(|src, root| {
            let g = glyph_named(root, glyph)?;
            Ok(edit::set_field(src, g.syntax(), name, &coord(value)))
        })?;
        what.push(format!("ADD {name}: {}", coord(value)));
    }
    // The origin guide moves against the bearing.
    let (what_moved, moved) = match edge {
        Edge::Left => ("origin guide", -delta),
        Edge::Right => ("advance guide", delta),
        Edge::Ink => ("ink", delta),
    };
    let what = if what.is_empty() {
        "no change".to_string()
    } else {
        what.join(", ")
    };
    s.summary = Some(format!(
        "{what} · {what_moved} Δ {}",
        signed(moved, "+", "\u{2212}"),
    ));
    Ok(())
}

/// `delta` in whole units after `plus` or `minus`: `+= 15`, `−40`.
fn signed(delta: f64, plus: &str, minus: &str) -> String {
    let sign = if delta.round() < 0.0 { minus } else { plus };
    format!("{sign}{}", coord(delta.abs()))
}

/// A relationship tool's rewrite of the point `let` at `range` (plan 5,
/// §1.4). Refused for a top-level `let`, and when the new expression
/// would make the point depend on itself.
fn relate(
    s: &mut Session,
    range: std::ops::Range<usize>,
    relation: &Relation,
) -> OpResult<()> {
    let (text, refs): (String, Vec<&str>) = match relation {
        Relation::Coincident { b } => (b.clone(), vec![b]),
        Relation::Meet { l1, l2 } => (
            format!("meet({}, {})", l1.text(), l2.text()),
            l1.names().into_iter().chain(l2.names()).collect(),
        ),
        Relation::Project { at, line } => (
            format!("project({}, {})", point_text(*at), line.text()),
            line.names(),
        ),
        Relation::Fraction { a, b, t } => (
            format!("mediate({a}, {b}, {t:.3})"),
            vec![a.as_str(), b.as_str()],
        ),
        Relation::Polar { q, len, angle } => (
            format!("polar({q}, {}, {angle:.1}deg)", coord(*len)),
            vec![q.as_str()],
        ),
        Relation::Mirror { q, axis } => (
            format!("mirror({q}, {})", axis.text()),
            std::iter::once(q.as_str()).chain(axis.names()).collect(),
        ),
    };
    s.step(|_, root| {
        let node = decl_at(root, range).ok_or("That point is no longer in the source.")?;
        if node.kind() != SyntaxKind::LET_STMT {
            return Err("Select a point `let`.".to_string());
        }
        let glyph = node
            .ancestors()
            .find_map(ast::Glyph::cast)
            .ok_or("Top-level lets are never rewritten by a tool.")?;
        let target = name_of(&node).ok_or("This `let` has no name.")?;
        if let Some(name) = refs.iter().find(|r| depends_on(&glyph, r, &target)) {
            return Err(format!("`{name}` depends on `{target}`: that would be a cycle."));
        }
        let value = ast::LetStmt::cast(node)
            .and_then(|l| l.value())
            .ok_or("This `let` has no value.")?;
        Ok(vec![edit::replace_expr(&value, &text)])
    })
}

/// Whether local `name` in `glyph` is, or depends through the glyph's
/// `let`s on, `target`.
fn depends_on(glyph: &ast::Glyph, name: &str, target: &str) -> bool {
    let Some(body) = glyph.body() else {
        return name == target;
    };
    let lets: std::collections::HashMap<String, Vec<String>> = body
        .items()
        .filter_map(ast::LetStmt::cast)
        .filter_map(|l| {
            let name = l.name_token()?.text().to_string();
            let deps = l
                .value()
                .map(|v| {
                    v.syntax()
                        .descendants()
                        .filter_map(ast::IdentExpr::cast)
                        .filter_map(|i| i.token().map(|t| t.text().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            Some((name, deps))
        })
        .collect();
    let mut stack = vec![name.to_string()];
    let mut seen = HashSet::new();
    while let Some(n) = stack.pop() {
        if n == target {
            return true;
        }
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(deps) = lets.get(&n) {
            stack.extend(deps.iter().cloned());
        }
    }
    false
}

/// Fails the op if the text no longer parses (a typed value that isn't
/// an expression).
fn check_parses(s: &Session, message: &str) -> OpResult<()> {
    let parsed = mg_syntax::parse(&s.src);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == mg_diag::Severity::Error)
    {
        return Err(message.to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Numbers

/// A raw-unit coordinate: whole units (plan 5, "Literal precision").
fn coord(v: f64) -> String {
    let r = v.round();
    if r == 0.0 { "0".to_string() } else { format!("{r}") }
}

fn point_text(p: Pt) -> String {
    format!("({}, {})", coord(p[0]), coord(p[1]))
}

/// A typed number: as short as it can be, up to 3 decimals.
fn number(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

// ---------------------------------------------------------------------
// Lookups

fn is_segment(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;
    matches!(kind, START | LINE | QUAD | CUBE | ARC | CLOSE)
}

fn is_decl(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;
    is_segment(kind)
        || matches!(
            kind,
            LET_STMT | PARAM | METRIC | GLYPH | INSTANCE | GROUP | KERN | PATH | ANCHOR | COMPONENT | FONT
        )
}

/// The declaration whose trivia-trimmed range is exactly `range`.
fn decl_at(root: &SyntaxNode, range: std::ops::Range<usize>) -> Option<SyntaxNode> {
    root.descendants()
        .filter(|n| is_decl(n.kind()))
        .find(|n| edit::node_range(n) == range)
}

fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::IDENT)
}

fn name_of(node: &SyntaxNode) -> Option<String> {
    name_token(node).map(|t| t.text().to_string())
}

fn text_of(src: &str, node: &SyntaxNode) -> String {
    src[edit::node_range(node)].to_string()
}

/// The default-set glyph `name` (no `glyphset:` field).
fn glyph_named(root: &SyntaxNode, name: &str) -> OpResult<ast::Glyph> {
    root.children()
        .filter_map(ast::Glyph::cast)
        .find(|g| {
            g.name_token().is_some_and(|t| t.text() == name)
                && edit::find_field(g.syntax(), "glyphset").is_none()
        })
        .ok_or_else(|| format!("There is no glyph `{name}`."))
}

fn enclosing_glyph_name(node: &SyntaxNode) -> Option<String> {
    node.ancestors()
        .find_map(ast::Glyph::cast)
        .and_then(|g| g.name_token())
        .map(|t| t.text().to_string())
}

fn path_named(glyph: &ast::Glyph, name: &str) -> Option<ast::Path> {
    glyph
        .body()?
        .items()
        .filter_map(ast::Path::cast)
        .find(|p| p.name_token().is_some_and(|t| t.text() == name))
}

fn last_segment(path: &ast::Path) -> Option<SyntaxNode> {
    path.body()?
        .items()
        .filter(|n| is_segment(n.kind()) && n.kind() != SyntaxKind::CLOSE)
        .last()
}

fn has_close(path: &ast::Path) -> bool {
    path.body()
        .is_some_and(|b| b.items().any(|n| n.kind() == SyntaxKind::CLOSE))
}

/// Every name a new glyph-local declaration must avoid: the glyph's own
/// `let`s, paths and anchors (one namespace), and the top level (which a
/// local would shadow, spec §5.11).
fn taken_names(root: &SyntaxNode, glyph: &ast::Glyph) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in root.children() {
        if matches!(item.kind(), SyntaxKind::LET_STMT | SyntaxKind::PARAM | SyntaxKind::METRIC)
            && let Some(n) = name_of(&item)
        {
            names.insert(n);
        }
    }
    if let Some(body) = glyph.body() {
        for item in body.items() {
            if let Some(n) = name_of(&item) {
                names.insert(n);
            }
        }
    }
    names
}

/// The first of `prefix0`, `prefix1`, … that is free in `glyph` (plan 5,
/// §1.3).
fn fresh(root: &SyntaxNode, glyph: &ast::Glyph, prefix: &str) -> String {
    let taken = taken_names(root, glyph);
    (0..)
        .map(|n| format!("{prefix}{n}"))
        .find(|n| !taken.contains(n))
        .expect("some name is free")
}

/// `name` if it is free in `glyph`, else `name2`, `name3`, ….
fn fresh_named(root: &SyntaxNode, glyph: &ast::Glyph, name: &str) -> String {
    let taken = taken_names(root, glyph);
    if !taken.contains(name) {
        return name.to_string();
    }
    (2..)
        .map(|n| format!("{name}{n}"))
        .find(|n| !taken.contains(n))
        .expect("some name is free")
}

/// Adds `let <prefixN> = <value>;` to `glyph`, returning the name.
fn add_let(
    s: &mut Session,
    glyph: &str,
    prefix: &str,
    value: impl FnOnce(&str) -> OpResult<String>,
) -> OpResult<String> {
    let mut name = String::new();
    s.step(|src, root| {
        let g = glyph_named(root, glyph)?;
        name = fresh(root, &g, prefix);
        let text = format!("let {name} = {};", value(&name)?);
        let body = g.body().ok_or("This glyph has no body.")?;
        Ok(vec![edit::insert_let(src, &body, &text)])
    })?;
    Ok(name)
}

/// Adds `let <name> = <value>;` (renamed if `name` is taken).
fn add_named_let(s: &mut Session, glyph: &str, name: &str, value: String) -> OpResult<String> {
    let mut out = String::new();
    s.step(|src, root| {
        let g = glyph_named(root, glyph)?;
        out = fresh_named(root, &g, name);
        let body = g.body().ok_or("This glyph has no body.")?;
        Ok(vec![edit::insert_let(src, &body, &format!("let {out} = {value};"))])
    })?;
    Ok(out)
}

/// `stroke`, `caps` and `joins` of `path`, as config text.
fn copied_config(path: &ast::Path) -> String {
    let src = path.syntax().ancestors().last().expect("a root").to_string();
    ["stroke", "caps", "joins"]
        .iter()
        .filter_map(|name| {
            let value = edit::find_field(path.syntax(), name)?.value()?;
            Some(format!("{name}: {}", text_of(&src, value.syntax())))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A new path after the glyph's last path (or its last item), separated
/// by a blank line.
fn insert_path(src: &str, body: &ast::Body, name: &str, config: &str, start: &str) -> TextEdit {
    let config = if config.is_empty() {
        String::new()
    } else {
        format!(" ({config})")
    };
    let after = body
        .items()
        .filter(|n| n.kind() == SyntaxKind::PATH)
        .last()
        .or_else(|| body.items().last());
    let Some(after) = after else {
        let text = format!("path {name}{config} {{\n    start (at: {start})\n}}");
        return edit::insert_decl(src, body, None, &text);
    };
    let range = edit::node_range(&after);
    let line_start = src[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = src[line_start..range.start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let text = format!(
        "\n\n{indent}path {name}{config} {{\n{indent}    start (at: {start})\n{indent}}}"
    );
    TextEdit {
        range: range.end..range.end,
        text,
    }
}

/// `segment` as a new line at the end of `path`, before its `close`.
fn append_segment(src: &str, path: &ast::Path, segment: &str) -> OpResult<TextEdit> {
    let body = path.body().ok_or("The path has no body.")?;
    let after = last_segment(path);
    Ok(edit::insert_decl(src, &body, after.as_ref(), segment))
}

/// Local `let`s that `segment`'s fields name directly and nothing else
/// uses: its end point and controls, created with it.
fn lets_only_used_by(root: &SyntaxNode, segment: &SyntaxNode) -> Vec<SyntaxNode> {
    let file = ast::SourceFile::cast(root.clone()).expect("the root is a SOURCE_FILE");
    let index = Index::new(&file);
    let seg: std::ops::Range<usize> = segment.text_range().into();
    let mut out = Vec::new();
    for ident in segment
        .descendants()
        .filter_map(ast::IdentExpr::cast)
        .filter(|i| i.syntax().parent().is_some_and(|p| p.kind() == SyntaxKind::FIELD))
    {
        let Some(token) = ident.token() else { continue };
        let Some(def @ Def::GlyphLocal { .. }) = index.resolve(&token) else {
            continue;
        };
        let Some(decl) = index.decl(&def) else { continue };
        let refs = index.references(root, &def);
        let outside = refs.iter().filter(|r| {
            let inside = seg.start <= r.start && r.end <= seg.end;
            !inside && r.start != decl.name_range.start
        });
        if outside.count() == 0
            && let Some(node) = root
                .descendants()
                .filter(|n| n.kind() == SyntaxKind::LET_STMT)
                .find(|n| edit::node_range(n) == decl.range)
        {
            out.push(node);
        }
    }
    out
}

/// Rewrites a segment as another kind (plan 5, §1.4).
fn set_segment_kind(
    s: &mut Session,
    range: std::ops::Range<usize>,
    kind: &str,
    controls: &[Pt],
) -> OpResult<()> {
    const GONE: &str = "That segment is no longer in the source.";
    let (glyph, to_text, to_name, name) = {
        let root = mg_syntax::parse(&s.src).syntax();
        let node = decl_at(&root, range.clone()).ok_or(GONE)?;
        if !matches!(
            node.kind(),
            SyntaxKind::LINE | SyntaxKind::QUAD | SyntaxKind::CUBE | SyntaxKind::ARC
        ) {
            return Err("Only line, quad, cube and arc segments change kind.".to_string());
        }
        let to = edit::find_field(&node, "to")
            .and_then(|f| f.value())
            .ok_or("This segment has no `to`.")?;
        let to_name = match &to {
            ast::Expr::Ident(i) => i.token().map(|t| t.text().to_string()),
            _ => None,
        };
        (
            enclosing_glyph_name(&node).ok_or(GONE)?,
            text_of(&s.src, to.syntax()),
            to_name,
            name_of(&node),
        )
    };
    let base = to_name.unwrap_or_else(|| name.clone().unwrap_or_else(|| "seg".to_string()));
    let control = |i: usize| controls.get(i).copied().ok_or("Missing control point positions.");

    // New control `let`s first, then the segment rewrite. The lets go at
    // the end of the glyph's `let`s, before its paths, so the segment
    // shifts by exactly their length.
    let len_before = s.src.len();
    let (fields, keyword) = match kind {
        "line" => (format!("to: {to_text}"), "line"),
        "quad" => {
            let c = add_named_let(s, &glyph, &format!("{base}_c"), point_text(control(0)?))?;
            (format!("c: {c}, to: {to_text}"), "quad")
        }
        "cube" => {
            let c1 = add_named_let(s, &glyph, &format!("{base}_c1"), point_text(control(0)?))?;
            let c2 = add_named_let(s, &glyph, &format!("{base}_c2"), point_text(control(1)?))?;
            (format!("c1: {c1}, c2: {c2}, to: {to_text}"), "cube")
        }
        "arc" => {
            let c = add_named_let(s, &glyph, &format!("{base}_ctr"), point_text(control(0)?))?;
            (format!("to: {to_text}, center: {c}, sweep: \"ccw\""), "arc")
        }
        other => return Err(format!("`{other}` is not a segment kind.")),
    };
    let text = match &name {
        Some(n) => format!("{keyword} {n} ({fields})"),
        None => format!("{keyword:<5} ({fields})"),
    };

    let shift = s.src.len() - len_before;
    s.step(|src, root| {
        let node = decl_at(root, range.start + shift..range.end + shift)
            .filter(|n| is_segment(n.kind()))
            .ok_or(GONE)?;
        let mut edits = vec![TextEdit {
            range: edit::node_range(&node),
            text: text.clone(),
        }];
        // Old control `let`s nothing else uses go with the old fields.
        for decl in lets_only_used_by(root, &node) {
            let used_by_new = decl_name_in(&decl, &text);
            if !used_by_new {
                edits.push(edit::remove_decl(src, &decl));
            }
        }
        Ok(edits)
    })
}

/// Whether the `let` `decl` is named in `text` (a rewritten segment).
fn decl_name_in(decl: &SyntaxNode, text: &str) -> bool {
    name_of(decl).is_some_and(|n| {
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|w| w == n)
    })
}

/// A rename's edits, or why it can't happen: the name must be valid and
/// not collide with a declaration it would duplicate or shadow (spec
/// §5.4, §5.11).
fn rename(root: &SyntaxNode, node: &SyntaxNode, name: &str) -> OpResult<Vec<TextEdit>> {
    let token = name_token(node).ok_or("This declaration has no name to change.")?;
    if token.text() == name {
        return Ok(Vec::new());
    }
    if let Some(why) = edit::invalid_name(name) {
        return Err(format!("`{name}` is {why}."));
    }
    let file = ast::SourceFile::cast(root.clone()).expect("the root is a SOURCE_FILE");
    let index = Index::new(&file);
    let def = index
        .resolve(&token)
        .ok_or("This declaration can't be renamed.")?;
    let taken = |d: Def| index.decl(&d).is_some();
    let glyph_name = |g: usize| index.glyphs[g].decl.name.clone();

    let conflict = match &def {
        Def::TopLevel(_) => {
            if taken(Def::TopLevel(name.to_string())) {
                Some(format!("`{name}` is already declared at the top level."))
            } else {
                (0..index.glyphs.len())
                    .find(|&g| {
                        taken(Def::GlyphLocal {
                            glyph: g,
                            name: name.to_string(),
                        })
                    })
                    .map(|g| format!("glyph {} already declares a local `{name}`.", glyph_name(g)))
            }
        }
        Def::GlyphLocal { glyph, .. } => {
            if taken(Def::GlyphLocal {
                glyph: *glyph,
                name: name.to_string(),
            }) {
                Some(format!("glyph {} already declares `{name}`.", glyph_name(*glyph)))
            } else if taken(Def::TopLevel(name.to_string())) {
                Some(format!("`{name}` would shadow a top-level declaration."))
            } else {
                None
            }
        }
        Def::Glyph(_) | Def::Group(_) => (taken(Def::Glyph(name.to_string()))
            || taken(Def::Group(name.to_string())))
        .then(|| format!("A glyph or group named `{name}` already exists.")),
        Def::Segment { glyph, path, .. } => taken(Def::Segment {
            glyph: *glyph,
            path: *path,
            name: name.to_string(),
        })
        .then(|| format!("This path already has a segment named `{name}`.")),
        Def::GlyphSet(_) => Some("Glyph sets can't be renamed here.".to_string()),
    };
    if let Some(message) = conflict {
        return Err(message);
    }
    Ok(edit::rename(root, &index, &def, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "let h = 1000;\nglyph A (advance: h) {\n    let stem0 = (0, 0);\n    let stem1 = (1, 1);\n\n    path stem (stroke: 50, caps: \"round\") {\n        start (at: stem0)\n        line  (to: stem1)\n    }\n}\n\nglyph B (advance: h) {\n}\n";

    const STEM: &str = "path stem (stroke: 50, caps: \"round\") {\n        start (at: stem0)\n        line  (to: stem1)\n    }";

    fn span_in(src: &str, text: &str) -> [usize; 2] {
        let start = src.find(text).unwrap_or_else(|| panic!("{text}"));
        [start, start + text.len()]
    }

    fn span_of(text: &str) -> [usize; 2] {
        span_in(SRC, text)
    }

    /// Applies an op's steps in order; checks the result parses.
    fn run_on(src: &str, op: &Op) -> (String, Option<Created>) {
        let result = run(src, 1, op);
        let EditResult::Ok { steps, created, .. } = result else {
            panic!("{result:?}");
        };
        let mut text = src.to_string();
        for step in steps {
            let edits: Vec<TextEdit> = step
                .into_iter()
                .map(|c| TextEdit {
                    range: c.from..c.to,
                    text: c.insert,
                })
                .collect();
            text = edit::apply(&text, &edits);
        }
        let errors = mg_syntax::parse(&text).diagnostics;
        assert!(errors.is_empty(), "{text}\n{errors:?}");
        (text, created)
    }

    fn applied(op: &Op) -> String {
        run_on(SRC, op).0
    }

    #[test]
    fn renames_a_local_and_its_uses() {
        let op = Op::Rename {
            span: span_of("let stem0 = (0, 0);"),
            name: "base".into(),
        };
        assert_eq!(applied(&op), SRC.replace("stem0", "base"));
    }

    #[test]
    fn refuses_bad_and_colliding_names() {
        let local = span_of("let stem0 = (0, 0);");
        for (name, message) in [
            ("glyph", "`glyph` is a reserved word."),
            ("2x", "`2x` is not an identifier."),
            ("stem1", "glyph A already declares `stem1`."),
            ("h", "`h` would shadow a top-level declaration."),
        ] {
            let op = Op::Rename {
                span: local,
                name: name.into(),
            };
            assert_eq!(
                run(SRC, 1, &op),
                EditResult::Invalid {
                    message: message.into()
                },
                "{name}"
            );
        }
    }

    #[test]
    fn deletes_a_declaration() {
        let op = Op::Delete {
            span: span_of("let stem1 = (1, 1);"),
        };
        assert_eq!(applied(&op), SRC.replace("    let stem1 = (1, 1);\n", ""));
    }

    #[test]
    fn deleting_a_segment_takes_its_own_point() {
        let op = Op::Delete {
            span: span_of("line  (to: stem1)"),
        };
        assert_eq!(
            applied(&op),
            SRC.replace("    let stem1 = (1, 1);\n", "")
                .replace("        line  (to: stem1)\n", "")
        );
    }

    #[test]
    fn refuses_text_with_syntax_errors() {
        let broken = format!("{SRC}glyph (");
        let op = Op::Delete { span: [0, 13] };
        assert_eq!(run(&broken, 1, &op), EditResult::ReadOnly);
    }

    #[test]
    fn adds_points_with_fresh_names() {
        let op = Op::AddPoint {
            glyph: "A".into(),
            at: [12.4, -30.6],
        };
        let (text, created) = run_on(SRC, &op);
        assert_eq!(
            text,
            SRC.replace("(1, 1);\n", "(1, 1);\n    let p0 = (12, -31);\n")
        );
        assert_eq!(
            created,
            Some(Created {
                kind: "point",
                name: "p0".into()
            })
        );
        // `p0` is now taken.
        let (again, created) = run_on(&text, &op);
        assert!(again.contains("let p1 = (12, -31);"));
        assert_eq!(created.unwrap().name, "p1");
    }

    #[test]
    fn adds_lines_and_measurements() {
        for (line, expected) in [
            (
                LineSpec::Through {
                    a: "stem0".into(),
                    b: "stem1".into(),
                },
                "let l0 = lineThrough(stem0, stem1);",
            ),
            (LineSpec::Hline { y: 333.3 }, "let l0 = hline(333);"),
            (LineSpec::Vline { x: -5.0 }, "let l0 = vline(-5);"),
        ] {
            let op = Op::AddLine {
                glyph: "A".into(),
                line,
            };
            assert!(applied(&op).contains(expected), "{expected}");
        }
        let op = Op::AddMeasure {
            glyph: "A".into(),
            a: "stem0".into(),
            b: "stem1".into(),
        };
        assert!(applied(&op).contains("let d0 = length(stem1 - stem0);"));
    }

    #[test]
    fn path_tool_start_append_close() {
        // A new path in an empty glyph is a construction path.
        let start = Op::PathStart {
            glyph: "B".into(),
            at: [10.0, 20.0],
            copy_from: None,
        };
        let (text, created) = run_on(SRC, &start);
        assert!(
            text.contains("glyph B (advance: h) {\n    let p0 = (10, 20);\n\n    path path0 {\n        start (at: p0)\n    }\n}\n"),
            "{text}"
        );
        assert_eq!(created.unwrap().name, "path0");

        let line = Op::PathAppend {
            glyph: "B".into(),
            path: "path0".into(),
            at: [100.0, 20.0],
            c1: None,
            c2: None,
        };
        let (text, _) = run_on(&text, &line);
        let cube = Op::PathAppend {
            glyph: "B".into(),
            path: "path0".into(),
            at: [100.0, 200.0],
            c1: Some([150.0, 60.0]),
            c2: Some([150.0, 160.0]),
        };
        let (text, _) = run_on(&text, &cube);
        // After a cube, `c1` is left to reflection.
        let smooth = Op::PathAppend {
            glyph: "B".into(),
            path: "path0".into(),
            at: [10.0, 200.0],
            c1: Some([0.0, 0.0]),
            c2: Some([40.0, 240.0]),
        };
        let (text, _) = run_on(&text, &smooth);
        let close = Op::PathClose {
            glyph: "B".into(),
            path: "path0".into(),
        };
        let (text, _) = run_on(&text, &close);
        assert!(
            text.ends_with("glyph B (advance: h) {\n    let p0 = (10, 20);\n    let p1 = (100, 20);\n    let p2 = (100, 200);\n    let p2_c1 = (150, 60);\n    let p2_c2 = (150, 160);\n    let p3 = (10, 200);\n    let p3_c2 = (40, 240);\n\n    path path0 {\n        start (at: p0)\n        line  (to: p1)\n        cube  (c1: p2_c1, c2: p2_c2, to: p2)\n        cube  (c2: p3_c2, to: p3)\n        close\n    }\n}\n"),
            "{text}"
        );
        // Closing twice is a no-op.
        assert_eq!(run_on(&text, &close).0, text);
    }

    #[test]
    fn new_paths_copy_stroke_caps_joins() {
        let op = Op::PathStart {
            glyph: "A".into(),
            at: [0.0, 500.0],
            copy_from: Some("stem".into()),
        };
        let (text, _) = run_on(SRC, &op);
        assert!(
            text.contains("        line  (to: stem1)\n    }\n\n    path path0 (stroke: 50, caps: \"round\") {\n        start (at: p0)\n    }\n}"),
            "{text}"
        );
    }

    #[test]
    fn segment_kinds_round_trip() {
        let to_cube = Op::SetSegmentKind {
            span: span_of("line  (to: stem1)"),
            kind: "cube".into(),
            controls: vec![[0.0, 1.0], [1.0, 0.0]],
        };
        let (cube, _) = run_on(SRC, &to_cube);
        assert!(
            cube.contains("    let stem1_c1 = (0, 1);\n    let stem1_c2 = (1, 0);\n"),
            "{cube}"
        );
        assert!(
            cube.contains("        cube  (c1: stem1_c1, c2: stem1_c2, to: stem1)\n"),
            "{cube}"
        );

        // Back to a line: its control lets go with it.
        let span = span_in(&cube, "cube  (c1: stem1_c1, c2: stem1_c2, to: stem1)");
        let to_line = Op::SetSegmentKind {
            span,
            kind: "line".into(),
            controls: vec![],
        };
        assert_eq!(run_on(&cube, &to_line).0, SRC);

        let to_arc = Op::SetSegmentKind {
            span: span_of("line  (to: stem1)"),
            kind: "arc".into(),
            controls: vec![[0.5, 0.5]],
        };
        let (arc, _) = run_on(SRC, &to_arc);
        assert!(
            arc.contains("        arc   (to: stem1, center: stem1_ctr, sweep: \"ccw\")\n"),
            "{arc}"
        );
    }

    #[test]
    fn path_fields() {
        let path = span_of(STEM);
        let joins = Op::SetField {
            span: path,
            name: "joins".into(),
            value: FieldValue::Str("bevel".into()),
        };
        assert!(applied(&joins).contains("(stroke: 50, caps: \"round\", joins: \"bevel\")"));
        let stroke = Op::SetField {
            span: path,
            name: "stroke".into(),
            value: FieldValue::Expr("h / 20".into()),
        };
        assert!(applied(&stroke).contains("(stroke: h / 20, caps"));
        let bad = Op::SetField {
            span: path,
            name: "stroke".into(),
            value: FieldValue::Expr("h +".into()),
        };
        assert!(matches!(run(SRC, 1, &bad), EditResult::Invalid { .. }));
        let caps = Op::RemoveField {
            span: path,
            name: "caps".into(),
        };
        assert!(applied(&caps).contains("path stem (stroke: 50) {"));

        let fill = Op::SetFill {
            span: path,
            on: true,
        };
        let (filled, _) = run_on(SRC, &fill);
        let closed = "path stem (stroke: 50, caps: \"round\", fill: true) {\n        start (at: stem0)\n        line  (to: stem1)\n        close\n    }";
        assert!(filled.contains(closed), "{filled}");
        let unfill = Op::SetFill {
            span: span_in(&filled, closed),
            on: false,
        };
        let (unfilled, _) = run_on(&filled, &unfill);
        assert!(unfilled.contains("(stroke: 50, caps: \"round\") {\n        start (at: stem0)\n        line  (to: stem1)\n        close\n    }"));
    }

    #[test]
    fn relationship_tools_rewrite_one_let() {
        let stem1 = span_of("let stem1 = (1, 1);");
        let cases: Vec<(Relation, &str)> = vec![
            (Relation::Coincident { b: "stem0".into() }, "let stem1 = stem0;"),
            (
                Relation::Fraction {
                    a: "stem0".into(),
                    b: "stem0".into(),
                    t: 0.56249,
                },
                "let stem1 = mediate(stem0, stem0, 0.562);",
            ),
            (
                Relation::Polar {
                    q: "stem0".into(),
                    len: 266.5,
                    angle: 143.04,
                },
                "let stem1 = polar(stem0, 267, 143.0deg);",
            ),
            (
                Relation::Project {
                    at: [1.0, 1.0],
                    line: LineRef::Through {
                        through: ["stem0".into(), "stem0".into()],
                    },
                },
                "let stem1 = project((1, 1), lineThrough(stem0, stem0));",
            ),
        ];
        for (relation, expected) in cases {
            let op = Op::Relate {
                span: stem1,
                relation,
            };
            let (text, _) = run_on(SRC, &op);
            assert!(text.contains(expected), "{expected}\n{text}");
        }
    }

    #[test]
    fn relationship_tools_refuse_cycles_and_top_level() {
        // stem0 would become `stem1`, and stem1 is then made `stem0`.
        let src = SRC.replace("let stem0 = (0, 0);", "let stem0 = stem1;");
        let op = Op::Relate {
            span: span_in(&src, "let stem1 = (1, 1);"),
            relation: Relation::Coincident { b: "stem0".into() },
        };
        assert_eq!(
            run(&src, 1, &op),
            EditResult::Invalid {
                message: "`stem0` depends on `stem1`: that would be a cycle.".into()
            }
        );
        let top = Op::Relate {
            span: span_of("let h = 1000;"),
            relation: Relation::Coincident { b: "x".into() },
        };
        assert!(matches!(run(SRC, 1, &top), EditResult::Invalid { .. }));
    }

    #[test]
    fn components() {
        let op = Op::AddComponent {
            glyph: "B".into(),
            target: "A".into(),
            offset: [40.0, 0.0],
        };
        let (text, _) = run_on(SRC, &op);
        assert!(
            text.ends_with("glyph B (advance: h) {\n    component (glyph: A, offset: (40, 0))\n}\n"),
            "{text}"
        );
        let own = Op::AddComponent {
            glyph: "B".into(),
            target: "B".into(),
            offset: [0.0, 0.0],
        };
        assert!(matches!(run(SRC, 1, &own), EditResult::Invalid { .. }));
    }

    const SPACED: &str = "font (name: \"T\", em: 1000)\nmetric xHeight (y: 500, overshoot: 10)\nlet side = 40;\nglyph a (advance: 500) {\n}\nglyph b (rsb: side) {\n}\nglyph c (lsb: 30) {\n}\nglyph d (lsb: 30, rsb: 20) {\n}\nglyph e (advance: 500, lsb: side) {\n}\nglyph f (advance: 500, rsb: 20) {\n}\n";

    /// The `glyph <name> (…)` header after a spacing op.
    fn spaced(name: &str, edge: Edge, delta: f64) -> (String, Option<String>) {
        let op = Op::Spacing {
            glyph: name.into(),
            edge,
            delta,
            lsb: 30.0,
            rsb: 20.0,
            em: 1000.0,
        };
        let EditResult::Ok { summary, .. } = run(SPACED, 1, &op) else {
            panic!("{name}");
        };
        let (text, _) = run_on(SPACED, &op);
        let header = text
            .lines()
            .find(|l| l.starts_with(&format!("glyph {name} ")))
            .unwrap()
            .to_string();
        (header, summary)
    }

    #[test]
    fn spacing_guide_drags_follow_the_table() {
        use Edge::*;
        let cases = [
            ("a", Left, "glyph a (advance: 515, lsb: 45) {"),
            ("a", Right, "glyph a (advance: 515) {"),
            ("a", Ink, "glyph a (advance: 500, lsb: 45) {"),
            ("b", Left, "glyph b (rsb: side, lsb: 45) {"),
            ("b", Right, "glyph b (rsb: side + 15) {"),
            ("b", Ink, "glyph b (rsb: side - 15, lsb: 45) {"),
            ("c", Left, "glyph c (lsb: 45, rsb: 20) {"),
            ("c", Right, "glyph c (lsb: 30, rsb: 35) {"),
            ("c", Ink, "glyph c (lsb: 45, rsb: 5) {"),
            ("d", Left, "glyph d (lsb: 45, rsb: 20) {"),
            ("d", Right, "glyph d (lsb: 30, rsb: 35) {"),
            ("d", Ink, "glyph d (lsb: 45, rsb: 5) {"),
            ("e", Left, "glyph e (advance: 515, lsb: side + 15) {"),
            ("e", Right, "glyph e (advance: 515, lsb: side) {"),
            ("e", Ink, "glyph e (advance: 500, lsb: side + 15) {"),
            ("f", Left, "glyph f (advance: 515, rsb: 20) {"),
            ("f", Right, "glyph f (advance: 515, rsb: 35) {"),
            ("f", Ink, "glyph f (advance: 500, rsb: 5) {"),
        ];
        for (name, edge, want) in cases {
            assert_eq!(spaced(name, edge, 15.0).0, want, "{name} {edge:?}");
        }
    }

    #[test]
    fn spacing_summaries_name_the_change() {
        assert_eq!(
            spaced("a", Edge::Left, 15.0).1.as_deref(),
            Some("advance += 15, ADD lsb: 45 · origin guide Δ \u{2212}15")
        );
        assert_eq!(
            spaced("f", Edge::Right, -40.0).1.as_deref(),
            Some("advance -= 40, rsb -= 40 · advance guide Δ \u{2212}40")
        );
        assert_eq!(
            spaced("f", Edge::Ink, 10.0).1.as_deref(),
            Some("rsb -= 10 · ink Δ +10")
        );
    }

    #[test]
    fn a_trailing_constant_returns_to_the_original() {
        let op = |delta| Op::Spacing {
            glyph: "b".into(),
            edge: Edge::Right,
            delta,
            lsb: 0.0,
            rsb: 40.0,
            em: 1000.0,
        };
        let (once, _) = run_on(SPACED, &op(15.0));
        let (back, _) = run_on(&once, &op(-15.0));
        assert_eq!(back, SPACED);
    }

    #[test]
    fn add_constant_to_a_metric() {
        let op = Op::AddConstant {
            span: span_in(SPACED, "metric xHeight (y: 500, overshoot: 10)"),
            name: "y".into(),
            delta: -12.0,
            em: 1000.0,
        };
        let (text, _) = run_on(SPACED, &op);
        assert!(
            text.contains("metric xHeight (y: 488, overshoot: 10)"),
            "{text}"
        );
        let missing = Op::AddConstant {
            span: span_in(SPACED, "metric xHeight (y: 500, overshoot: 10)"),
            name: "align".into(),
            delta: 1.0,
            em: 1000.0,
        };
        assert!(matches!(
            run(SPACED, 1, &missing),
            EditResult::Invalid { .. }
        ));
    }

    #[test]
    fn font_fields() {
        let set = |name: &str, value: Option<FieldValue>| {
            run_on(
                SPACED,
                &Op::FontField {
                    name: name.into(),
                    value,
                },
            )
            .0
        };
        let designer = set("designer", Some(FieldValue::Str("Ann \"A\" B".into())));
        assert!(
            designer.starts_with("font (name: \"T\", em: 1000, designer: \"Ann \\\"A\\\" B\")\n"),
            "{designer}"
        );
        let renamed = set("name", Some(FieldValue::Str("U".into())));
        assert!(renamed.starts_with("font (name: \"U\", em: 1000)\n"));
        let em = set("em", Some(FieldValue::Num(2048.0)));
        assert!(em.starts_with("font (name: \"T\", em: 2048)\n"));
        let removed = run_on(
            &designer,
            &Op::FontField {
                name: "designer".into(),
                value: None,
            },
        )
        .0;
        assert_eq!(removed, SPACED);
    }
}
