//! Local inverse drag (plan 5, §1.5): dragging a point rewrites one
//! numeric literal upstream of it, so the point follows the pointer.
//!
//! - **Drivers** are the literals in the point's own expression (right to
//!   left: a trailing offset first), then in the glyph-local `let`s it
//!   references (breadth-first),
//!   stopping at top-level names: a drag never changes a top-level `let`,
//!   param or metric.
//! - Each axis is driven by the first literal that moves it; a key cycles
//!   to the next. When one literal drives both axes (an angle), the point
//!   moves along its track instead.
//! - **Solving** evaluates the point with the literal replaced, reusing
//!   every other value from the drag-start evaluation: a closed form when
//!   the point moves linearly with the literal, otherwise a bracketed
//!   search, limited to ±2× the literal's magnitude so it stays continuous
//!   with the start.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::rc::Rc;

use indexmap::IndexMap;
use mg_eval::NodeId;
use mg_eval::value::Value;
use mg_syntax::ast::{self, AstNode};
use mg_syntax::edit::{self, TextEdit};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use serde::Serialize;

use crate::doc::Model;
use crate::offsets::Utf16Index;
use crate::ops::Change;

type Pt = [f64; 2];

/// Sensitivities below this move nothing.
const EPS: f64 = 1e-6;
/// How close counts as reaching the pointer, in font units.
const HIT: f64 = 0.5;

#[derive(Debug, Clone)]
struct Driver {
    /// The literal token's byte range in the drag-start source.
    range: Range<usize>,
    text: String,
    /// The `let` whose expression holds the literal.
    owner: String,
    /// Its value in its own unit, signed (a leading unary `-` included).
    value: f64,
    negated: bool,
    /// How far the point moves per unit of the literal, per axis.
    sens: Pt,
    linear: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverInfo {
    pub literal: String,
    pub owner: String,
    pub value: f64,
    pub sens: Pt,
    pub linear: bool,
}

/// What dragging a point would change, for the DRIVERS panel and the drag
/// tooltip.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragInfo {
    pub target: String,
    pub at: Pt,
    pub drivers: Vec<DriverInfo>,
    /// The driver for x and for y (indices into `drivers`); `None` for a
    /// locked axis.
    pub axis: [Option<usize>; 2],
    /// One driver moves both axes: the point follows its track.
    pub track: bool,
    /// For a track: where the point goes across the driver's range, to
    /// draw during the drag.
    pub track_points: Vec<Pt>,
    /// For a locked axis: the top-level names it depends on.
    pub locked_by: [Vec<String>; 2],
    /// Other points the target is placed from: a drag never moves them
    /// (they are dragged directly), so they lock what they set.
    pub anchors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragStep {
    /// Changes to the drag-start text (UTF-16 offsets).
    pub changes: Vec<Change>,
    /// Where the point ends up.
    pub at: Pt,
    /// Each moved driver's new literal text.
    pub literals: Vec<(usize, String)>,
    pub exact: bool,
    /// A driver stopped at its extreme: going further would make the
    /// source invalid.
    pub limited: bool,
}

/// A drag in progress, on the drag-start evaluation.
pub struct Session {
    model: Rc<Model>,
    instance: String,
    glyph: String,
    target: String,
    root: SyntaxNode,
    /// The glyph-local `let`s the target depends on, dependencies first,
    /// ending with the target.
    order: Vec<String>,
    drivers: Vec<Driver>,
    axis: [Option<usize>; 2],
    at: Pt,
    /// The target and the local `let`s searched for drivers.
    searched: Vec<String>,
    /// Other points the target is placed from; never moved by its drag.
    anchors: Vec<String>,
    /// The instance's dependency graph, for re-evaluating what a changed
    /// literal feeds (a literal change never changes it).
    graph: mg_eval::Graph,
    /// Errors at the drag start, per set of drivers changed.
    baselines: RefCell<HashMap<Vec<usize>, usize>>,
    /// Whether each tried set of rounded values is valid.
    validity: RefCell<HashMap<Vec<(usize, u64)>, bool>>,
    /// Each driver's last valid value in this drag: where the next step
    /// moves from.
    last_valid: RefCell<HashMap<usize, f64>>,
}

impl Session {
    /// Analyzes dragging point `target` of `glyph`. `prefer` is the
    /// driver last chosen per axis for this point, if any.
    pub fn begin(
        model: Rc<Model>,
        instance: &str,
        glyph: &str,
        target: &str,
        prefer: [Option<usize>; 2],
    ) -> Result<Session, String> {
        let decl = model
            .hir
            .glyphs
            .get(&(glyph.to_string(), None))
            .ok_or_else(|| format!("There is no glyph `{glyph}`."))?;
        let values = &model
            .outcomes
            .get(instance)
            .ok_or("There is no such instance.")?
            .values;
        let at = match values.get(&NodeId::GlyphLocal(glyph.to_string(), target.to_string())) {
            Some(Value::Pair(p)) => [p.x, p.y],
            _ => return Err(format!("`{target}` is not a point with a value.")),
        };
        let root = decl.syntax.ancestors().last().expect("a root");

        // Local lets upstream of the target, breadth-first (driver order),
        // and dependencies-first (evaluation order).
        let local = |name: &str| decl.lets.contains_key(name);
        let mut bfs = vec![target.to_string()];
        let mut seen: HashSet<String> = HashSet::from([target.to_string()]);
        // Another point is a handle of its own, dragged directly: the
        // search stops there, as at a top-level name. It goes on through
        // numbers and lines (`bar_y = hline(0.333 * h)`).
        let is_point = |name: &str| {
            matches!(
                values.get(&NodeId::GlyphLocal(glyph.to_string(), name.to_string())),
                Some(Value::Pair(_))
            )
        };
        let mut anchors = Vec::new();
        let mut queue = VecDeque::from([target.to_string()]);
        while let Some(name) = queue.pop_front() {
            for dep in idents(&decl.lets[&name].value) {
                if local(&dep) && seen.insert(dep.clone()) {
                    if is_point(&dep) {
                        anchors.push(dep);
                        continue;
                    }
                    bfs.push(dep.clone());
                    queue.push_back(dep);
                }
            }
        }
        let mut order = Vec::new();
        let mut done = HashSet::new();
        fn visit(
            name: &str,
            lets: &IndexMap<String, mg_hir::model::LetDecl>,
            done: &mut HashSet<String>,
            order: &mut Vec<String>,
        ) {
            if !done.insert(name.to_string()) {
                return;
            }
            for dep in idents(&lets[name].value) {
                if lets.contains_key(&dep) {
                    visit(&dep, lets, done, order);
                }
            }
            order.push(name.to_string());
        }
        visit(target, &decl.lets, &mut done, &mut order);

        let mut drivers = Vec::new();
        for owner in &bfs {
            let Some(expr) = &decl.lets[owner].value else {
                continue;
            };
            // Right to left within an expression: in `base - delta`, the
            // trailing offset is the one to adjust (plan 5's own test:
            // `180deg - 37deg` drags `37deg`).
            let mut tokens: Vec<SyntaxToken> = expr
                .syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| edit::literal_value(t).is_some())
                .collect();
            tokens.reverse();
            for token in tokens {
                let negated = token
                    .parent()
                    .and_then(|lit| lit.parent())
                    .and_then(ast::UnaryExpr::cast)
                    .and_then(|u| u.op_token())
                    .is_some_and(|op| op.kind() == SyntaxKind::MINUS);
                let magnitude = edit::literal_value(&token).expect("filtered");
                drivers.push(Driver {
                    range: token.text_range().into(),
                    text: token.text().to_string(),
                    owner: owner.clone(),
                    value: if negated { -magnitude } else { magnitude },
                    negated,
                    sens: [0.0, 0.0],
                    linear: false,
                });
            }
        }

        let graph = mg_eval::graph::build(
            &model.hir,
            model
                .hir
                .instances
                .get(instance)
                .ok_or("There is no such instance.")?,
        );
        let mut session = Session {
            model,
            instance: instance.to_string(),
            glyph: glyph.to_string(),
            target: target.to_string(),
            root,
            order,
            searched: bfs,
            anchors,
            graph,
            baselines: RefCell::default(),
            validity: RefCell::default(),
            last_valid: RefCell::default(),
            drivers,
            axis: [None, None],
            at,
        };
        session.measure();
        session.axis = [0, 1].map(|a| {
            let moves = |i: usize| session.drivers[i].sens[a].abs() > EPS;
            prefer[a]
                .filter(|&i| i < session.drivers.len() && moves(i))
                .or_else(|| (0..session.drivers.len()).find(|&i| moves(i)))
        });
        Ok(session)
    }

    /// Each driver's sensitivity and linearity, by finite differences.
    fn measure(&mut self) {
        for i in 0..self.drivers.len() {
            let v = self.drivers[i].value;
            let h = (v.abs() * 1e-3).max(1e-3);
            let (Some(a), Some(b), Some(c)) = (
                self.eval(&[(i, v - h)]),
                self.eval(&[(i, v + h)]),
                self.eval(&[(i, v + 3.0 * h)]),
            ) else {
                continue;
            };
            let sens = [(b[0] - a[0]) / (2.0 * h), (b[1] - a[1]) / (2.0 * h)];
            // Linear when the slope over [v+h, v+3h] matches.
            let far = [(c[0] - b[0]) / (2.0 * h), (c[1] - b[1]) / (2.0 * h)];
            let linear = (0..2).all(|k| (far[k] - sens[k]).abs() <= 1e-6 * sens[k].abs().max(1.0));
            self.drivers[i].sens = sens;
            self.drivers[i].linear = linear;
        }
    }

    pub fn info(&self) -> DragInfo {
        let track = matches!(self.axis, [Some(x), Some(y)] if x == y);
        let locked_by = [0, 1].map(|a| {
            if self.axis[a].is_some() {
                return Vec::new();
            }
            self.top_level_names()
        });
        let track_points = match (track, self.axis[0]) {
            (true, Some(i)) => {
                let v = self.drivers[i].value;
                let reach = 2.0 * v.abs().max(1.0);
                (0..=64)
                    .filter_map(|k| self.eval(&[(i, v - reach + reach * k as f64 / 32.0)]))
                    .collect()
            }
            _ => Vec::new(),
        };
        DragInfo {
            target: self.target.clone(),
            at: self.at,
            track_points,
            drivers: self
                .drivers
                .iter()
                .map(|d| DriverInfo {
                    literal: d.text.clone(),
                    owner: d.owner.clone(),
                    value: d.value,
                    sens: d.sens,
                    linear: d.linear,
                })
                .collect(),
            axis: self.axis,
            track,
            locked_by,
            anchors: self.anchors.clone(),
        }
    }

    /// Top-level names the searched `let`s use: with the anchors, what
    /// locks an axis with no local driver.
    fn top_level_names(&self) -> Vec<String> {
        let hir = &self.model.hir;
        let decl = &hir.glyphs[&(self.glyph.clone(), None)];
        let mut out = Vec::new();
        for name in &self.searched {
            for dep in idents(&decl.lets[name].value) {
                let top = !decl.lets.contains_key(&dep)
                    && (hir.lets.contains_key(&dep)
                        || hir.params.contains_key(&dep)
                        || hir.metrics.contains_key(&dep));
                if top && !out.contains(&dep) {
                    out.push(dep);
                }
            }
        }
        out
    }

    /// Moves axis `axis`'s driver to the next literal that moves it.
    pub fn cycle(&mut self, axis: usize) {
        let n = self.drivers.len();
        let start = self.axis[axis].map_or(0, |i| i + 1);
        self.axis[axis] = (0..n)
            .map(|k| (start + k) % n)
            .find(|&i| self.drivers[i].sens[axis].abs() > EPS)
            .or(self.axis[axis]);
    }

    /// The target's value with drivers overridden, or `None` if that fails
    /// to evaluate (a domain error, say).
    fn eval(&self, overrides: &[(usize, f64)]) -> Option<Pt> {
        let model = &*self.model;
        let hir = &model.hir;
        let instance = hir.instances.get(&self.instance)?;
        let values = &model.outcomes.get(&self.instance)?.values;
        let decl = &hir.glyphs[&(self.glyph.clone(), None)];
        let mut computed: IndexMap<NodeId, Value> = IndexMap::new();
        let changed: HashSet<&str> = overrides
            .iter()
            .map(|(i, _)| self.drivers[*i].owner.as_str())
            .collect();
        let mut dirty: HashSet<&str> = HashSet::new();

        for name in &self.order {
            let expr = decl.lets[name].value.as_ref()?;
            let is_dirty = changed.contains(name.as_str())
                || idents(&decl.lets[name].value)
                    .iter()
                    .any(|d| dirty.contains(d.as_str()));
            if !is_dirty {
                continue;
            }
            dirty.insert(name.as_str());
            let value = if changed.contains(name.as_str()) {
                let text = self.substituted(expr, name, overrides);
                let parsed = mg_syntax::parse(&format!("let __drag = {text};"));
                let expr = parsed
                    .syntax()
                    .descendants()
                    .find_map(ast::LetStmt::cast)
                    .and_then(|l| l.value())?;
                mg_eval::eval_subexpr_with(
                    hir,
                    instance,
                    Some(&self.glyph),
                    values,
                    &computed,
                    &expr,
                )?
            } else {
                mg_eval::eval_subexpr_with(
                    hir,
                    instance,
                    Some(&self.glyph),
                    values,
                    &computed,
                    expr,
                )?
            };
            computed.insert(NodeId::GlyphLocal(self.glyph.clone(), name.clone()), value);
        }
        let node = NodeId::GlyphLocal(self.glyph.clone(), self.target.clone());
        match computed.get(&node).or_else(|| values.get(&node))? {
            Value::Pair(p) => Some([p.x, p.y]),
            _ => None,
        }
    }

    /// `expr` (the `let` `owner`'s) as text, with its overridden literals
    /// replaced by their values at full precision.
    fn substituted(&self, expr: &ast::Expr, owner: &str, overrides: &[(usize, f64)]) -> String {
        let range = edit::node_range(expr.syntax());
        let mut text = self.model.source[range.clone()].to_string();
        let mut mine: Vec<(Range<usize>, String)> = overrides
            .iter()
            .filter(|(i, _)| self.drivers[*i].owner == owner)
            .map(|(i, v)| {
                let d = &self.drivers[*i];
                let (_, suffix) = split_unit(&d.text);
                // Inside a unary `-`, the literal holds the magnitude; a
                // parenthesized signed value keeps any sign correct.
                let literal = if d.negated { -v } else { *v };
                (d.range.clone(), format!("({literal}{suffix})"))
            })
            .collect();
        mine.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (r, value) in mine {
            text.replace_range(r.start - range.start..r.end - range.start, &value);
        }
        text
    }

    /// The driver value that puts axis `axis` of the point at `target`
    /// (`None` axis: the whole point at `pointer`, along the track).
    fn solve(
        &self,
        driver: usize,
        axis: Option<usize>,
        pointer: Pt,
        fixed: &[(usize, f64)],
    ) -> f64 {
        let d = &self.drivers[driver];
        let with = |v: f64| {
            let mut o = fixed.to_vec();
            o.push((driver, v));
            self.eval(&o)
        };
        let miss = |v: f64| -> f64 {
            match with(v) {
                Some(p) => match axis {
                    Some(a) => (p[a] - pointer[a]).abs(),
                    None => (p[0] - pointer[0]).hypot(p[1] - pointer[1]),
                },
                None => f64::INFINITY,
            }
        };

        // Linear: closed form.
        if let (Some(a), true) = (axis, d.linear)
            && let Some(p0) = with(d.value)
        {
            let s = d.sens[a];
            if s.abs() > EPS {
                let v = d.value + (pointer[a] - p0[a]) / s;
                if miss(v) <= HIT {
                    return v;
                }
            }
        }

        // Otherwise: golden-section search on the miss, then secant
        // refinement of the signed error for an axis.
        let reach = 2.0 * d.value.abs().max(1.0);
        let (mut lo, mut hi) = (d.value - reach, d.value + reach);
        let phi = (5f64.sqrt() - 1.0) / 2.0;
        let mut a = hi - phi * (hi - lo);
        let mut b = lo + phi * (hi - lo);
        let (mut fa, mut fb) = (miss(a), miss(b));
        for _ in 0..40 {
            if fa < fb {
                hi = b;
                b = a;
                fb = fa;
                a = hi - phi * (hi - lo);
                fa = miss(a);
            } else {
                lo = a;
                a = b;
                fa = fb;
                b = lo + phi * (hi - lo);
                fb = miss(b);
            }
        }
        let mut best = if fa < fb { a } else { b };
        if let Some(ax) = axis {
            let err = |v: f64| with(v).map(|p| p[ax] - pointer[ax]);
            let (mut x0, mut x1) = (best, best + (reach * 1e-3).max(1e-6));
            for _ in 0..8 {
                let (Some(e0), Some(e1)) = (err(x0), err(x1)) else {
                    break;
                };
                if (e1 - e0).abs() < 1e-12 {
                    break;
                }
                let x2 = x1 - e1 * (x1 - x0) / (e1 - e0);
                if !x2.is_finite() || miss(x2) > miss(best) + 1e-9 {
                    break;
                }
                best = x2;
                x0 = x1;
                x1 = x2;
            }
        }
        best
    }

    /// The step for dragging to `pointer`: each driven axis solved (x
    /// first, then y given x), literals rounded to their decimal counts,
    /// and each held at the last valid value past an extreme.
    pub fn drag_to(&self, pointer: Pt) -> DragStep {
        let mut values: Vec<(usize, f64)> = Vec::new();
        let mut limited = false;
        let axes: Vec<(usize, Option<usize>)> = match self.axis {
            [Some(x), Some(y)] if x == y => vec![(x, None)],
            [x, y] => [(x, Some(0)), (y, Some(1))]
                .into_iter()
                .filter_map(|(d, a)| d.map(|d| (d, a)))
                .collect(),
        };
        for (driver, axis) in axes {
            let solved = self.solve(driver, axis, pointer, &values);
            let (v, hit_limit) = self.clamp(driver, solved, &values);
            limited |= hit_limit;
            values.push((driver, v));
        }
        let mut step = self.step(&values, pointer);
        step.limited = limited;
        step
    }

    /// The step for setting driver `driver` to `value` (the scrub slider),
    /// held at the last valid value past an extreme.
    pub fn set(&self, driver: usize, value: f64) -> DragStep {
        let (v, limited) = self.clamp(driver, value, &[]);
        let at = self.eval(&[(driver, v)]).unwrap_or(self.at);
        let mut step = self.step(&[(driver, v)], at);
        step.limited = limited;
        step
    }

    /// `value` rounded to the driver's decimal count: what its literal
    /// will say.
    fn rounded(&self, driver: usize, value: f64) -> f64 {
        let places = self.places(driver);
        let factor = 10f64.powi(places as i32);
        (value * factor).round() / factor
    }

    fn places(&self, driver: usize) -> usize {
        let (number, _) = split_unit(&self.drivers[driver].text);
        number.split_once('.').map_or(0, |(_, f)| f.len())
    }

    /// `value` for `driver` (given `fixed`), or, if that would put the
    /// source into an invalid state, the valid value closest to it between
    /// the drag-start value and it: the drag bottoms out at the driver's
    /// extreme. The flag says it did.
    fn clamp(&self, driver: usize, value: f64, fixed: &[(usize, f64)]) -> (f64, bool) {
        let target = self.rounded(driver, value);
        let with = |v: f64| {
            let mut all = fixed.to_vec();
            all.push((driver, v));
            all
        };
        // Moving from the last valid value, the whole way must be valid,
        // not just the end: a jump never skips over an invalid stretch.
        let from = self.last_valid.borrow().get(&driver).copied();
        let from = from.unwrap_or(self.drivers[driver].value);
        let mut first_bad = None;
        for k in 1..=8 {
            let v = self.rounded(driver, from + (target - from) * k as f64 / 8.0);
            if !self.valid(&with(v)) {
                first_bad = Some(v);
                break;
            }
        }
        let Some(bad) = first_bad else {
            self.last_valid.borrow_mut().insert(driver, target);
            return (target, false);
        };
        // Bisect on the literal's own grid, from the last valid value
        // towards the first invalid one.
        let step = 10f64.powi(-(self.places(driver) as i32));
        let (mut lo, mut hi) = (from, bad);
        for _ in 0..40 {
            if (hi - lo).abs() <= step * 1.5 {
                break;
            }
            let mid = self.rounded(driver, (lo + hi) / 2.0);
            if mid == lo || mid == hi {
                break;
            }
            if self.valid(&with(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.last_valid.borrow_mut().insert(driver, lo);
        (lo, true)
    }

    /// Whether the source with `values` in place is no worse than at the
    /// drag start: it parses, lowers, and re-evaluating what the changed
    /// `let`s feed (paths and their strokes included) reports no more
    /// errors than the unchanged text does. Errors already there don't
    /// block a drag; new ones (a zero-length segment, a curvature radius
    /// below half the stroke, a domain error) do.
    fn valid(&self, values: &[(usize, f64)]) -> bool {
        let key: Vec<(usize, u64)> = values
            .iter()
            .map(|&(i, v)| (i, self.rounded(i, v).to_bits()))
            .collect();
        if let Some(&known) = self.validity.borrow().get(&key) {
            return known;
        }
        let mut drivers: Vec<usize> = values.iter().map(|(i, _)| *i).collect();
        drivers.sort_unstable();
        drivers.dedup();
        let baseline = {
            let cached = self.baselines.borrow().get(&drivers).copied();
            match cached {
                Some(b) => Some(b),
                None => {
                    let start: Vec<(usize, f64)> = drivers
                        .iter()
                        .map(|&i| (i, self.drivers[i].value))
                        .collect();
                    let b = self.errors(&start);
                    if let Some(b) = b {
                        self.baselines.borrow_mut().insert(drivers.clone(), b);
                    }
                    b
                }
            }
        };
        let ok = match (self.errors(values), baseline) {
            (Some(now), Some(base)) => now <= base,
            _ => false,
        };
        self.validity.borrow_mut().insert(key, ok);
        ok
    }

    /// Errors re-evaluating the drivers' `let`s (and all they feed) with
    /// `values` in the text; `None` if it doesn't even lower.
    fn errors(&self, values: &[(usize, f64)]) -> Option<usize> {
        let mut edits: Vec<TextEdit> = Vec::new();
        for &(i, v) in values {
            let token = token_at(&self.root, &self.drivers[i].range)?;
            edits.extend(edit::replace_literal(&token, v));
        }
        let text = edit::apply(&self.model.source, &edits);
        let parsed = mg_syntax::parse(&text);
        let is_error = |d: &mg_diag::Diagnostic| d.severity == mg_diag::Severity::Error;
        if parsed.diagnostics.iter().any(is_error) {
            return None;
        }
        let file = ast::SourceFile::cast(parsed.syntax())?;
        let (hir, diagnostics) = mg_hir::lower(&file);
        if diagnostics.iter().any(is_error) {
            return None;
        }
        let instance = hir.instances.get(&self.instance)?;
        let previous = self.model.outcomes.get(&self.instance)?;
        let mut owners: Vec<&str> = values
            .iter()
            .map(|(i, _)| self.drivers[*i].owner.as_str())
            .collect();
        owners.sort_unstable();
        owners.dedup();
        let mut current: Option<mg_eval::EvalOutcome> = None;
        let mut errors = 0;
        for owner in owners {
            let node = NodeId::GlyphLocal(self.glyph.clone(), owner.to_string());
            let outcome = mg_eval::reevaluate(
                &hir,
                instance,
                &self.graph,
                current.as_ref().unwrap_or(previous),
                &node,
            );
            errors += outcome.diagnostics.iter().filter(|d| is_error(d)).count();
            current = Some(outcome);
        }
        Some(errors)
    }

    /// Rounds `values` to their literals' decimal counts and turns them
    /// into edits of the drag-start text.
    fn step(&self, values: &[(usize, f64)], pointer: Pt) -> DragStep {
        let mut edits: Vec<TextEdit> = Vec::new();
        let mut rounded = Vec::new();
        let mut literals = Vec::new();
        for &(i, v) in values {
            let d = &self.drivers[i];
            let Some(token) = token_at(&self.root, &d.range) else {
                continue;
            };
            let changes = edit::replace_literal(&token, v);
            // The value the rounded text actually means.
            let (number, _) = split_unit(&d.text);
            let places = number.split_once('.').map_or(0, |(_, f)| f.len());
            let factor = 10f64.powi(places as i32);
            rounded.push((i, (v * factor).round() / factor));
            let lit = changes
                .iter()
                .find(|c| c.range == d.range)
                .map_or_else(|| d.text.clone(), |c| c.text.clone());
            literals.push((i, lit));
            edits.extend(changes);
        }
        let at = self.eval(&rounded).unwrap_or(self.at);
        let exact = (at[0] - pointer[0]).abs() <= HIT && (at[1] - pointer[1]).abs() <= HIT;
        let offsets = Utf16Index::new(&self.model.source);
        DragStep {
            changes: edits
                .into_iter()
                .map(|e| Change {
                    from: offsets.convert(e.range.start),
                    to: offsets.convert(e.range.end),
                    insert: e.text,
                })
                .collect(),
            at,
            literals,
            exact,
            limited: false,
        }
    }
}

/// Identifiers referenced by an optional expression.
fn idents(expr: &Option<ast::Expr>) -> Vec<String> {
    let Some(expr) = expr else { return Vec::new() };
    expr.syntax()
        .descendants()
        .filter_map(ast::IdentExpr::cast)
        // A call's callee is a function, not a binding.
        .filter(|i| {
            i.syntax()
                .parent()
                .and_then(ast::CallExpr::cast)
                .and_then(|c| c.callee())
                .is_none_or(|callee| callee.syntax() != i.syntax())
        })
        .filter_map(|i| i.token().map(|t| t.text().to_string()))
        .collect()
}

fn split_unit(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    (&text[..end], &text[end..])
}

fn token_at(root: &SyntaxNode, range: &Range<usize>) -> Option<SyntaxToken> {
    root.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| Range::<usize>::from(t.text_range()) == *range)
}

/// The driver chosen per axis, remembered per point for the session
/// (plan 5, §1.5).
pub type Preferences = HashMap<(String, String), [Option<usize>; 2]>;

#[cfg(test)]
mod tests {
    use super::*;

    /// A triangle `A` built from named points, and an `a` whose stem
    /// starts at a polar point on a centre-mode arc.
    const FIXTURE: &str = r#"font (name: "T", em: 1000)

let h = 1000;
let w = 500;

metric baseline  (y: 0)
metric xHeight   (y: 500)
metric capHeight (y: h)
metric ascender  (y: 1100)
metric descender (y: -250)

instance Regular ()

glyph A (advance: w) {
    let stem0 = (0, 0);
    let stem1 = (0.500 * w, h);
    let stem2 = (w, 0);

    let bar_y = hline(0.333 * h);

    let bar0 = meet(lineThrough(stem0, stem1), bar_y);
    let bar1 = meet(lineThrough(stem2, stem1), bar_y);

    path stem (stroke: 50, caps: "round", joins: "round") {
        start (at: stem0)
        line  (to: stem1)
        line  (to: stem2)
    }
    path bar (stroke: 50, caps: "round", joins: "round") {
        start (at: bar0)
        line  (to: bar1)
    }
}

glyph a (advance: w) {
    let arc_ctr = (0.500 * w, 0.400 * h);
    let arc_radius = 0.500 * w;
    let arc0 = polar(arc_ctr, arc_radius, 180deg - 37deg);

    path stem (stroke: 50, caps: "round", joins: "round") {
        start (at: arc0)
        arc   (center: arc_ctr, to: polar(arc_ctr, arc_radius, 25deg), sweep: "cw")
        line  (to: (0.975 * w, 0.085 * h))
    }
}
"#;

    fn session(glyph: &str, target: &str) -> Session {
        let model = crate::doc::analyze(FIXTURE, 0).1.expect("evaluates");
        Session::begin(Rc::new(model), "Regular", glyph, target, [None, None]).expect("drag")
    }

    /// The source after applying a step.
    fn after(s: &Session, step: &DragStep) -> String {
        let edits: Vec<TextEdit> = step
            .changes
            .iter()
            .map(|c| TextEdit {
                range: c.from..c.to,
                text: c.insert.clone(),
            })
            .collect();
        edit::apply(&s.model.source, &edits)
    }

    #[test]
    fn a_linear_driver_hits_the_pointer_exactly() {
        // `let stem1 = (0.500 * w, h);` with w = 500: x = 250.
        let s = session("A", "stem1");
        let info = s.info();
        assert_eq!(info.drivers.len(), 1);
        assert_eq!(info.drivers[0].literal, "0.500");
        assert!(info.drivers[0].linear);
        assert!((info.drivers[0].sens[0] - 500.0).abs() < 1e-6);
        assert_eq!(info.axis, [Some(0), None]);

        let step = s.drag_to([280.0, 1000.0]);
        assert!(step.exact, "{step:?}");
        assert!((step.at[0] - 280.0).abs() < 1e-9);
        assert!(after(&s, &step).contains("let stem1 = (0.560 * w, h);"));
    }

    #[test]
    fn an_axis_with_only_top_level_drivers_is_locked() {
        // The apex y is `h`: nothing local drives it.
        let s = session("A", "stem1");
        let info = s.info();
        assert_eq!(info.axis[1], None);
        assert_eq!(info.locked_by[1], ["w", "h"]);
        // Dragging vertically changes nothing on y.
        let step = s.drag_to([250.0, 400.0]);
        assert!((step.at[1] - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn a_polar_point_drags_its_angle_offset() {
        // `let arc0 = polar(arc_ctr, arc_radius, 180deg - 37deg);`
        let s = session("a", "arc0");
        let info = s.info();
        let driver = &info.drivers[info.axis[0].expect("x driven")];
        assert_eq!(driver.literal, "37deg");
        assert!(info.track, "one angle moves both axes");

        // Drag towards the angle 150° around the centre: only `37deg`
        // changes, to 30deg.
        let ctr = [0.5 * 500.0, 0.4 * 1000.0];
        let r = 0.5 * 500.0;
        let theta = 150f64.to_radians();
        let step = s.drag_to([ctr[0] + r * theta.cos(), ctr[1] + r * theta.sin()]);
        let text = after(&s, &step);
        assert!(
            text.contains("polar(arc_ctr, arc_radius, 180deg - 30deg)"),
            "{:?}",
            step.literals
        );
        assert_eq!(step.changes.len(), 1);
    }

    #[test]
    fn a_driver_upstream_in_another_let_is_found() {
        // `bar0 = meet(lineThrough(stem0, stem1), bar_y)`; `bar_y =
        // hline(0.333 * h)`: bar0's y is driven by 0.333, in `bar_y`.
        let s = session("A", "bar0");
        let info = s.info();
        let y = &info.drivers[info.axis[1].expect("y driven")];
        assert_eq!((y.literal.as_str(), y.owner.as_str()), ("0.333", "bar_y"));
        // It moves bar0 along the left leg, stem0 (0, 0) to stem1
        // (250, 1000): a track. At y = 400 the leg is at x = 100.
        assert!(info.track);
        let step = s.drag_to([100.0, 400.0]);
        assert!(after(&s, &step).contains("let bar_y = hline(0.400 * h);"));
    }

    #[test]
    fn other_points_are_anchors_not_drivers() {
        // `bar1 = meet(lineThrough(stem2, stem1), bar_y)`: stem1 and stem2
        // are handles of their own, so a drag of bar1 never moves them.
        // bar_y's 0.333 is its one driver: bar1 slides along the right leg,
        // stem2 (500, 0) to stem1 (250, 1000), reaching y = 400 at x = 400.
        let s = session("A", "bar1");
        let info = s.info();
        assert!(
            info.drivers.iter().all(|d| d.owner == "bar_y"),
            "{:?}",
            info.drivers
        );
        assert_eq!(info.anchors, ["stem2", "stem1"]);
        assert!(info.track);
        let step = s.drag_to([400.0, 400.0]);
        let text = after(&s, &step);
        assert!(text.contains("let stem1 = (0.500 * w, h);"));
        assert!(text.contains("let stem2 = (w, 0);"));
        assert!(text.contains("let bar_y = hline(0.400 * h);"));
    }

    #[test]
    fn a_polar_point_keeps_its_centre() {
        // `arc0 = polar(arc_ctr, arc_radius, …)`: the radius (a number)
        // can drive it; the centre (a point) can't.
        let s = session("a", "arc0");
        let info = s.info();
        assert_eq!(info.anchors, ["arc_ctr"]);
        assert!(info.drivers.iter().any(|d| d.owner == "arc_radius"));
        assert!(info.drivers.iter().all(|d| d.owner != "arc_ctr"));
    }

    #[test]
    fn a_drag_bottoms_out_before_the_source_becomes_invalid() {
        // `sqrt(k)` is a domain error below 0, so dragging `k` down stops
        // just above it, at the literal's own precision (0.001).
        let source = "font (name: \"T\", em: 1000)\nmetric baseline (y: 0)\nmetric xHeight (y: 500)\nmetric capHeight (y: 700)\nmetric ascender (y: 800)\nmetric descender (y: -200)\ninstance Regular ()\nglyph A (advance: 500) {\n    let k = 4.000;\n    let p = (sqrt(k) * 100, 0);\n}\n";
        let model = crate::doc::analyze(source, 0).1.expect("evaluates");
        let mut s = Session::begin(Rc::new(model), "Regular", "A", "p", [None, None]).unwrap();
        for _ in 0..s.info().drivers.len() {
            if s.info().drivers[s.info().axis[0].unwrap()].literal == "4.000" {
                break;
            }
            s.cycle(0);
        }
        let k = s.info().axis[0].unwrap();
        let step = s.set(k, -1.0);
        assert!(step.limited, "{step:?}");
        let (_, literal) = &step.literals[0];
        let value: f64 = literal.parse().unwrap();
        assert!((0.0..0.2).contains(&value), "stopped at {value}");
        // The value it stopped at is valid; past 0 is not.
        assert!(s.valid(&[(k, value)]));
        assert!(!s.valid(&[(k, -0.001)]));
    }

    #[test]
    fn a_valid_drag_is_not_limited() {
        let s = session("A", "stem1");
        let step = s.drag_to([280.0, 1000.0]);
        assert!(!step.limited);
        assert!(step.exact);
    }

    #[test]
    fn cycling_moves_to_the_next_driver() {
        let source = "font (name: \"T\", em: 1000)\nmetric baseline (y: 0)\nmetric xHeight (y: 500)\nmetric capHeight (y: 700)\nmetric ascender (y: 800)\nmetric descender (y: -200)\ninstance Regular ()\nglyph A (advance: 500) {\n    let k = 0.5;\n    let p = (k * 100 + 10, 0);\n}\n";
        let model = crate::doc::analyze(source, 0).1.expect("evaluates");
        let mut s = Session::begin(Rc::new(model), "Regular", "A", "p", [None, None]).unwrap();
        let literal = |s: &Session| s.info().drivers[s.info().axis[0].unwrap()].literal.clone();
        // Right to left in its own expression, then `k`'s.
        assert_eq!(literal(&s), "10");
        s.cycle(0);
        assert_eq!(literal(&s), "100");
        s.cycle(0);
        assert_eq!(literal(&s), "0.5");
        s.cycle(0);
        assert_eq!(literal(&s), "10", "wraps around");
    }
}
