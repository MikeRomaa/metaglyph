//! Per-node evaluation (spec §4), run once per instance in the order
//! [`crate::toposort`] produces. Mirrors `mg_hir::type_check`'s shape —
//! same walk over [`ast::Expr`], same "already diagnosed, propagate
//! silently" pattern — but computes real [`Value`]s instead of types, and
//! looks up a referenced binding's value in the (already-evaluated, by
//! construction) node cache instead of recursively inferring it.

use std::ops::Range;

use indexmap::{IndexMap, IndexSet};
use kurbo::{Affine, Point, Shape};
use mg_diag::{Diagnostic, Label};
use mg_hir::const_eval;
use mg_hir::model::{
    Align, ComponentDecl, GlyphDecl, Hir, InstanceDecl, MetricDecl, ParamDecl, SegmentDecl,
    SegmentKind, Sweep,
};
use mg_syntax::ast::{self, AstNode};
use mg_syntax::syntax_kind::SyntaxKind;

use crate::construct;
use crate::errors::EvalError;
use crate::graph::{self, Graph, NodeId};
use crate::toposort::{self, TopoOutcome};
use crate::value::{Rect, Value, Zone};
use mg_diag::codes;

pub struct EvalOutcome {
    pub values: IndexMap<NodeId, Value>,
    pub failed: IndexSet<NodeId>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Builds the graph and evaluates it, both for this one instance (spec
/// plan M3: cross-glyph references resolve per instance glyph set).
pub fn evaluate(hir: &Hir, instance: &InstanceDecl) -> (Graph, EvalOutcome) {
    let graph = graph::build(hir, instance);
    let outcome = run(hir, instance, &graph, &|| false).expect("never cancelled");
    (graph, outcome)
}

/// [`evaluate`], abandoned as soon as `cancelled` returns `true`. It is
/// checked between graph nodes, so an editor can drop a stale evaluation
/// the moment a new edit arrives (plan 4, L3). `None` when cancelled.
pub fn evaluate_cancellable(
    hir: &Hir,
    instance: &InstanceDecl,
    cancelled: &dyn Fn() -> bool,
) -> Option<(Graph, EvalOutcome)> {
    let graph = graph::build(hir, instance);
    let outcome = run(hir, instance, &graph, cancelled)?;
    Some((graph, outcome))
}

fn run(
    hir: &Hir,
    instance: &InstanceDecl,
    graph: &Graph,
    cancelled: &dyn Fn() -> bool,
) -> Option<EvalOutcome> {
    let TopoOutcome { sorted, remaining } = toposort::topo_sort(graph);
    let mut values: IndexMap<NodeId, Value> = IndexMap::new();
    let mut failed: IndexSet<NodeId> = IndexSet::new();
    let mut diagnostics = Vec::new();

    if !remaining.is_empty() {
        let cycle = toposort::find_cycle(graph, &remaining);
        diagnostics.push(cycle_diagnostic(hir, &cycle));
        failed.extend(remaining);
    }

    for node in &sorted {
        if cancelled() {
            return None;
        }
        eval_node(
            hir,
            instance,
            graph,
            node,
            &mut values,
            &mut failed,
            &mut diagnostics,
        );
    }

    Some(EvalOutcome {
        values,
        failed,
        diagnostics,
    })
}

fn eval_node(
    hir: &Hir,
    instance: &InstanceDecl,
    graph: &Graph,
    node: &NodeId,
    values: &mut IndexMap<NodeId, Value>,
    failed: &mut IndexSet<NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Failure containment (spec §4.6): a node downstream of any failure
    // fails silently, with no diagnostic of its own.
    if graph.deps[node].iter().any(|dep| failed.contains(dep)) {
        failed.insert(node.clone());
        return;
    }

    match compute_node(hir, instance, node, values, diagnostics) {
        Ok(value) => {
            values.insert(node.clone(), value);
        }
        Err(()) => {
            failed.insert(node.clone());
        }
    }
}

fn compute_node(
    hir: &Hir,
    instance: &InstanceDecl,
    node: &NodeId,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, ()> {
    match node {
        NodeId::TopLevel(name) => {
            if let Some(param) = hir.params.get(name) {
                return Ok(Value::Num(eval_param(hir, instance, name, param)));
            }
            if let Some(metric) = hir.metrics.get(name) {
                let mut ctx = EvalCtx::new(hir, instance, None, values, diagnostics);
                return eval_metric(&mut ctx, metric);
            }
            let let_decl = &hir.lets[name];
            let mut ctx = EvalCtx::new(hir, instance, None, values, diagnostics);
            eval_expr(&mut ctx, expr_of(&let_decl.value))
        }
        NodeId::GlyphLocal(glyph_name, name) => {
            let glyph = effective_glyph(hir, instance, glyph_name);
            let let_decl = &glyph.lets[name];
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            eval_expr(&mut ctx, expr_of(&let_decl.value))
        }
        NodeId::Anchor(glyph_name, name) => {
            let glyph = effective_glyph(hir, instance, glyph_name);
            let anchor = &glyph.anchors[name];
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            eval_expr(&mut ctx, expr_of(&anchor.at))
        }
        NodeId::GlyphAdvance(glyph_name) => {
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            eval_glyph_advance(&mut ctx, glyph_name).map(Value::Num)
        }
        NodeId::GlyphShift(glyph_name) => {
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            eval_glyph_shift(&mut ctx, glyph_name).map(Value::Num)
        }
        NodeId::Kern(i) => {
            let kern = &hir.kerns[*i];
            let mut ctx = EvalCtx::new(hir, instance, None, values, diagnostics);
            eval_expr(&mut ctx, expr_of(&kern.by))
        }
        NodeId::PathRealized(glyph_name, i) => {
            eval_path_realized(hir, instance, glyph_name, *i, values, diagnostics)
        }
        NodeId::PathBbox(glyph_name, i) => {
            eval_path_bbox(hir, instance, glyph_name, *i, values, diagnostics)
        }
        NodeId::GlyphBbox(glyph_name) => {
            eval_glyph_bbox(hir, instance, glyph_name, values, diagnostics)
        }
        NodeId::ComponentPath(glyph_name, i) => {
            eval_component_path(hir, instance, glyph_name, *i, values, diagnostics)
        }
        NodeId::ComponentBbox(glyph_name, i) => {
            let contours =
                render_path_component(hir, instance, glyph_name, *i, values, diagnostics)?;
            Ok(contours_bbox(&contours).map_or(Value::NoInk, Value::Rect))
        }
    }
}

/// A path component's skeleton (spec §5.7): its `path` expression's value,
/// transformed by its placement — exact for Béziers, so this is the
/// skeleton the component strokes, not an approximation of one.
fn eval_component_path(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    index: usize,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let component = &glyph.components[index];
    let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
    let source = eval_expr(&mut ctx, expr_of(&component.path))?;
    let affine = component_affine(&mut ctx, component)?;
    let skeleton = source
        .as_path()
        .expect("mg-hir already type-checked a component's `path`");
    let mut path = skeleton.path.clone();
    path.apply_affine(affine);
    Ok(Value::Path(mg_geom::skeleton::Skeleton {
        path,
        piece_counts: skeleton.piece_counts.clone(),
    }))
}

/// The union of `contours`' bounds; `None` when there are none.
fn contours_bbox(contours: &Contours) -> Option<Rect> {
    contours
        .iter()
        .map(|(contour, _)| contour.bounding_box())
        .reduce(|a, b| a.union(b))
        .map(Rect::from)
}

/// Every HIR field this evaluator reads is `Some` whenever the HIR has no
/// error diagnostics — a missing one there is already reported, and
/// `mg-eval` never runs on a broken HIR (see `mg-cli`'s wiring).
fn expr_of(field: &Option<ast::Expr>) -> &ast::Expr {
    field
        .as_ref()
        .expect("mg-hir guarantees this field on a clean HIR")
}

fn effective_glyph<'a>(hir: &'a Hir, instance: &InstanceDecl, name: &str) -> &'a GlyphDecl {
    graph::effective_glyph(hir, instance, name)
        .expect("mg-hir guarantees every alternate has a default")
}

/// A param's value (spec §5.6): the instance's override when it gives
/// one, else the declared default — both always constant expressions, so
/// this never needs the graph.
fn eval_param(hir: &Hir, instance: &InstanceDecl, name: &str, param: &ParamDecl) -> f64 {
    let font_em = hir.font.em.map(|v| v as f64);
    if let Some(over) = instance.overrides.get(name) {
        return const_eval::eval_const(over, font_em)
            .expect("mg-hir guarantees an override is constant");
    }
    param
        .default_value
        .expect("mg-hir guarantees a param has a constant default")
}

fn eval_metric(ctx: &mut EvalCtx, metric: &MetricDecl) -> Result<Value, ()> {
    let y = match &metric.y {
        Some(e) => value_num(ctx, e)?,
        None => 0.0,
    };
    let overshoot = match &metric.overshoot {
        Some(e) => value_num(ctx, e)?,
        None => 0.0,
    };
    let ink = match metric.align {
        Align::Top => y + overshoot,
        Align::Bottom => y - overshoot,
    };
    Ok(Value::Zone(Zone { y, overshoot, ink }))
}

// ---------------------------------------------------------------------
// Expression evaluation

pub(crate) struct EvalCtx<'a> {
    hir: &'a Hir,
    instance: &'a InstanceDecl,
    current_glyph: Option<&'a str>,
    values: &'a IndexMap<NodeId, Value>,
    /// Values that take precedence over `values`: an editor's
    /// what-if evaluation (see [`eval_subexpr_with`]).
    overrides: Option<&'a IndexMap<NodeId, Value>>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> EvalCtx<'a> {
    pub(crate) fn new(
        hir: &'a Hir,
        instance: &'a InstanceDecl,
        current_glyph: Option<&'a str>,
        values: &'a IndexMap<NodeId, Value>,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            hir,
            instance,
            current_glyph,
            values,
            overrides: None,
            diagnostics,
        }
    }

    fn fail(&mut self, span: Range<usize>, err: EvalError) -> Result<Value, ()> {
        self.diagnostics.push(diagnostic_for(span, err));
        Err(())
    }

    fn value_of(&self, node: &NodeId) -> Value {
        if let Some(value) = self.overrides.and_then(|o| o.get(node)) {
            return value.clone();
        }
        self.values
            .get(node)
            .cloned()
            .expect("this node's dependency is already evaluated by topological order")
    }
}

fn value_num(ctx: &mut EvalCtx, expr: &ast::Expr) -> Result<f64, ()> {
    Ok(eval_expr(ctx, expr)?
        .as_num()
        .expect("mg-hir already type-checked this as `num`"))
}

/// Evaluates `expr` against an evaluation's `values`, as if it appeared in
/// `glyph`'s body (`None`: top level). For reading a subexpression of a
/// binding that evaluated successfully, such as the origin argument of a
/// `polar` call, so an editor can draw it; every node `expr` references
/// must be in `values`, which holds for any subexpression of a binding
/// that has a value. Diagnostics are discarded.
pub fn eval_subexpr(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph: Option<&str>,
    values: &IndexMap<NodeId, Value>,
    expr: &ast::Expr,
) -> Option<Value> {
    let mut diagnostics = Vec::new();
    let mut ctx = EvalCtx::new(hir, instance, glyph, values, &mut diagnostics);
    eval_expr(&mut ctx, expr).ok()
}

/// [`eval_subexpr`], with `overrides` taking precedence over `values`:
/// what `expr` would be if those bindings had other values. An editor's
/// inverse drag (plan 5, §1.5) evaluates a glyph's local `let`s this way
/// with one literal changed, without re-evaluating the font. `expr` need
/// not come from the HIR's own syntax tree: names resolve by text.
pub fn eval_subexpr_with(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph: Option<&str>,
    values: &IndexMap<NodeId, Value>,
    overrides: &IndexMap<NodeId, Value>,
    expr: &ast::Expr,
) -> Option<Value> {
    let mut diagnostics = Vec::new();
    let mut ctx = EvalCtx::new(hir, instance, glyph, values, &mut diagnostics);
    ctx.overrides = Some(overrides);
    eval_expr(&mut ctx, expr).ok()
}

fn eval_expr(ctx: &mut EvalCtx, expr: &ast::Expr) -> Result<Value, ()> {
    use ast::Expr;
    match expr {
        Expr::Literal(lit) => eval_literal(ctx, lit),
        Expr::Ident(ident) => eval_ident(ctx, ident),
        Expr::Paren(paren) => match paren.inner() {
            Some(inner) => eval_expr(ctx, &inner),
            None => Err(()),
        },
        Expr::Tuple(tuple) => eval_tuple(ctx, tuple),
        Expr::List(list) => {
            let items: Result<Vec<Value>, ()> =
                list.elements().map(|e| eval_expr(ctx, &e)).collect();
            Ok(Value::List(items?))
        }
        Expr::Map(_) => {
            unreachable!("a map literal only ever fills `caps`/`joinAt`, read directly by mg-hir")
        }
        Expr::Unary(unary) => eval_unary(ctx, unary),
        Expr::Bin(bin) => eval_bin(ctx, bin),
        Expr::Call(call) => eval_call(ctx, call),
        Expr::Member(member) => eval_member(ctx, member),
        Expr::Range(_) => {
            unreachable!("range only ever fills `param range:`, read directly by mg-hir")
        }
        Expr::Error(_) => Err(()),
    }
}

fn eval_literal(ctx: &mut EvalCtx, lit: &ast::Literal) -> Result<Value, ()> {
    let token = lit.token().expect("a LITERAL node always wraps one token");
    match token.kind() {
        SyntaxKind::NUMBER
        | SyntaxKind::NUMBER_ANGLE
        | SyntaxKind::NUMBER_RATIO
        | SyntaxKind::NUMBER_HEX
        | SyntaxKind::NUMBER_CODEPOINT
        | SyntaxKind::NUMBER_CHAR => {
            let font_em = ctx.hir.font.em.map(|v| v as f64);
            let value = const_eval::literal_num_value(&token, font_em)
                .expect("mg-hir already validated this literal");
            Ok(Value::Num(value))
        }
        SyntaxKind::STRING => Ok(Value::String(lit.string_value().unwrap_or_default())),
        SyntaxKind::TRUE_KW => Ok(Value::Bool(true)),
        SyntaxKind::FALSE_KW => Ok(Value::Bool(false)),
        _ => unreachable!("not a literal token kind"),
    }
}

fn eval_ident(ctx: &mut EvalCtx, ident: &ast::IdentExpr) -> Result<Value, ()> {
    let token = ident.token().expect("an IDENT_EXPR always wraps one token");
    let name = token.text();

    if let Some(glyph_name) = ctx.current_glyph {
        let glyph = effective_glyph(ctx.hir, ctx.instance, glyph_name);
        if glyph.anchors.contains_key(name) {
            return Ok(ctx.value_of(&NodeId::Anchor(glyph_name.to_string(), name.to_string())));
        }
        if let Some(i) = glyph
            .paths
            .iter()
            .position(|p| p.name.as_deref() == Some(name))
        {
            return Ok(ctx.value_of(&NodeId::PathRealized(glyph_name.to_string(), i)));
        }
        if glyph.lets.contains_key(name) {
            return Ok(ctx.value_of(&NodeId::GlyphLocal(
                glyph_name.to_string(),
                name.to_string(),
            )));
        }
    }

    if ctx.hir.params.contains_key(name)
        || ctx.hir.metrics.contains_key(name)
        || ctx.hir.lets.contains_key(name)
    {
        return Ok(ctx.value_of(&NodeId::TopLevel(name.to_string())));
    }

    Ok(builtin_constant(name).unwrap_or_else(|| unreachable!("mg-hir already resolved `{name}`")))
}

fn builtin_constant(name: &str) -> Option<Value> {
    match name {
        "right" => Some(Value::Pair(Point::new(1.0, 0.0))),
        "up" => Some(Value::Pair(Point::new(0.0, 1.0))),
        "left" => Some(Value::Pair(Point::new(-1.0, 0.0))),
        "down" => Some(Value::Pair(Point::new(0.0, -1.0))),
        "identity" => Some(Value::Transform(Affine::IDENTITY)),
        _ => None,
    }
}

fn eval_tuple(ctx: &mut EvalCtx, tuple: &ast::TupleExpr) -> Result<Value, ()> {
    let elements: Result<Vec<Value>, ()> = tuple.elements().map(|e| eval_expr(ctx, &e)).collect();
    let elements = elements?;
    if elements.len() == 2 && elements.iter().all(|v| matches!(v, Value::Num(_))) {
        let mut nums = elements.into_iter().map(|v| v.as_num().unwrap());
        return Ok(Value::Pair(Point::new(
            nums.next().unwrap(),
            nums.next().unwrap(),
        )));
    }
    if elements.len() >= 2 && elements.iter().all(|v| matches!(v, Value::Transform(_))) {
        // Composes in reading order (spec §5.8): the first element's
        // transform applies first, so later ones pre-multiply it.
        let mut composed = Affine::IDENTITY;
        for v in elements {
            composed = v.as_transform().unwrap() * composed;
        }
        return Ok(Value::Transform(composed));
    }
    unreachable!("mg-hir already type-checked this tuple")
}

fn eval_unary(ctx: &mut EvalCtx, unary: &ast::UnaryExpr) -> Result<Value, ()> {
    let operand = eval_expr(ctx, &unary.operand().ok_or(())?)?;
    let op = unary.op_token().ok_or(())?;
    match (op.kind(), operand) {
        (SyntaxKind::MINUS, Value::Num(n)) => Ok(Value::Num(-n)),
        (SyntaxKind::MINUS, Value::Pair(p)) => Ok(Value::Pair(Point::new(-p.x, -p.y))),
        (SyntaxKind::NOT_KW, Value::Bool(b)) => Ok(Value::Bool(!b)),
        _ => unreachable!("mg-hir already type-checked this operand"),
    }
}

fn eval_bin(ctx: &mut EvalCtx, bin: &ast::BinExpr) -> Result<Value, ()> {
    let lhs = eval_expr(ctx, &bin.lhs().ok_or(())?)?;
    let op = bin.op_token().ok_or(())?;
    let rhs = eval_expr(ctx, &bin.rhs().ok_or(())?)?;
    let span: Range<usize> = bin.syntax().text_range().into();

    use SyntaxKind::*;
    match (lhs, rhs) {
        (Value::Num(a), Value::Num(b)) => match op.kind() {
            PLUS => Ok(Value::Num(a + b)),
            MINUS => Ok(Value::Num(a - b)),
            STAR => Ok(Value::Num(a * b)),
            SLASH => match construct::division(a, b) {
                Ok(v) => Ok(Value::Num(v)),
                Err(e) => ctx.fail(span, e),
            },
            CARET => match construct::power(a, b) {
                Ok(v) => Ok(Value::Num(v)),
                Err(e) => ctx.fail(span, e),
            },
            LT => Ok(Value::Bool(a < b)),
            LE => Ok(Value::Bool(a <= b)),
            GT => Ok(Value::Bool(a > b)),
            GE => Ok(Value::Bool(a >= b)),
            EQEQ => Ok(Value::Bool(a == b)),
            NEQ => Ok(Value::Bool(a != b)),
            _ => unreachable!(),
        },
        (Value::Pair(a), Value::Pair(b)) => match op.kind() {
            PLUS => Ok(Value::Pair(a + b.to_vec2())),
            MINUS => Ok(Value::Pair(a - b.to_vec2())),
            EQEQ => Ok(Value::Bool(a == b)),
            NEQ => Ok(Value::Bool(a != b)),
            _ => unreachable!(),
        },
        (Value::Pair(a), Value::Num(b)) => match op.kind() {
            STAR => Ok(Value::Pair(a.to_vec2().mul(b).to_point())),
            SLASH => match construct::division(1.0, b) {
                Ok(inv) => Ok(Value::Pair(a.to_vec2().mul(inv).to_point())),
                Err(e) => ctx.fail(span, e),
            },
            _ => unreachable!(),
        },
        (Value::Num(a), Value::Pair(b)) if op.kind() == STAR => {
            Ok(Value::Pair(b.to_vec2().mul(a).to_point()))
        }
        (Value::Bool(a), Value::Bool(b)) => match op.kind() {
            AND_KW => Ok(Value::Bool(a && b)),
            OR_KW => Ok(Value::Bool(a || b)),
            EQEQ => Ok(Value::Bool(a == b)),
            NEQ => Ok(Value::Bool(a != b)),
            _ => unreachable!(),
        },
        (Value::String(a), Value::String(b)) => match op.kind() {
            EQEQ => Ok(Value::Bool(a == b)),
            NEQ => Ok(Value::Bool(a != b)),
            _ => unreachable!(),
        },
        _ => unreachable!("mg-hir already type-checked these operand types"),
    }
}

fn eval_call(ctx: &mut EvalCtx, call: &ast::CallExpr) -> Result<Value, ()> {
    let ast::Expr::Ident(callee) = call.callee().ok_or(())? else {
        unreachable!("mg-hir only allows a plain function name in call position")
    };
    let name = callee
        .token()
        .expect("callee always has a token")
        .text()
        .to_string();
    let args: Vec<ast::Expr> = call
        .arg_list()
        .map_or_else(Vec::new, |l| l.args().collect());
    let values: Result<Vec<Value>, ()> = args.iter().map(|a| eval_expr(ctx, a)).collect();
    let values = values?;

    let span: Range<usize> = call.syntax().text_range().into();
    match construct::call(&name, &values, tolerances(ctx.hir).arc) {
        Ok(v) => Ok(v),
        Err(e) => ctx.fail(span, e),
    }
}

fn eval_member(ctx: &mut EvalCtx, member: &ast::MemberExpr) -> Result<Value, ()> {
    let receiver = member.receiver().ok_or(())?;
    let member_token = member.member_token().ok_or(())?;
    let field = member_token.text();

    if let ast::Expr::Ident(root) = &receiver
        && let Some(root_token) = root.token()
    {
        match root_token.text() {
            "math" => return Ok(Value::Num(math_constant(field))),
            "font" => return eval_font_member(ctx, field),
            "instance" => return Ok(eval_instance_member(ctx, field)),
            "glyph" => {
                let glyph_name = ctx
                    .current_glyph
                    .expect("mg-hir only allows `glyph.*` inside a glyph body")
                    .to_string();
                return eval_glyph_member(ctx, &glyph_name, field, member);
            }
            _ => {}
        }
    }

    if let ast::Expr::Member(inner) = &receiver
        && let Some(ast::Expr::Ident(root)) = inner.receiver()
        && root.token().as_ref().map(|t| t.text()) == Some("glyphs")
        && let Some(glyph_name_token) = inner.member_token()
    {
        return eval_glyph_member(ctx, glyph_name_token.text(), field, member);
    }

    let receiver_value = eval_expr(ctx, &receiver)?;
    Ok(match receiver_value {
        Value::Pair(p) => match field {
            "x" => Value::Num(p.x),
            "y" => Value::Num(p.y),
            _ => unreachable!(),
        },
        Value::Rect(r) => match field {
            "x0" => Value::Num(r.x0),
            "y0" => Value::Num(r.y0),
            "x1" => Value::Num(r.x1),
            "y1" => Value::Num(r.y1),
            "width" => Value::Num(r.width()),
            "height" => Value::Num(r.height()),
            "center" => Value::Pair(r.center()),
            _ => unreachable!(),
        },
        Value::Zone(z) => match field {
            "y" => Value::Num(z.y),
            "ink" => Value::Num(z.ink),
            "overshoot" => Value::Num(z.overshoot),
            _ => unreachable!(),
        },
        Value::Path(p) => match field {
            "bbox" => Value::Rect(construct::bbox(&p.path)),
            _ => unreachable!(),
        },
        Value::Ellipse(e) => match field {
            "center" => Value::Pair(e.center),
            "rx" => Value::Num(e.rx),
            "ry" => Value::Num(e.ry),
            _ => unreachable!(),
        },
        _ => unreachable!("mg-hir already type-checked this member access"),
    })
}

fn math_constant(name: &str) -> f64 {
    match name {
        "pi" => std::f64::consts::PI,
        "tau" => std::f64::consts::TAU,
        "e" => std::f64::consts::E,
        _ => unreachable!(),
    }
}

fn eval_font_member(ctx: &mut EvalCtx, field: &str) -> Result<Value, ()> {
    let font = &ctx.hir.font;
    Ok(match field {
        "name" => Value::String(font.name.clone().unwrap_or_default()),
        "em" => Value::Num(font.em.unwrap_or(0) as f64),
        "version" => Value::String(font.version.clone()),
        "designer" => Value::String(font.designer.clone().unwrap_or_default()),
        "foundry" => Value::String(font.foundry.clone().unwrap_or_default()),
        "license" => Value::String(font.license.clone().unwrap_or_default()),
        _ => unreachable!(),
    })
}

fn eval_instance_member(ctx: &EvalCtx, field: &str) -> Value {
    match field {
        "name" => Value::String(ctx.instance.name.clone()),
        "slant" => {
            let font_em = ctx.hir.font.em.map(|v| v as f64);
            let slant = ctx
                .instance
                .slant
                .as_ref()
                .map(|e| {
                    const_eval::eval_const(e, font_em)
                        .expect("mg-hir guarantees `slant` is constant")
                })
                .unwrap_or(0.0);
            Value::Num(slant)
        }
        _ => unreachable!(),
    }
}

/// `glyph.<field>` or `glyphs.<name>.<field>`, in that glyph's authored
/// coordinates either way: its shift places it in its own advance only
/// (spec §5.10).
fn eval_glyph_member(
    ctx: &mut EvalCtx,
    glyph_name: &str,
    field: &str,
    member: &ast::MemberExpr,
) -> Result<Value, ()> {
    match field {
        "advance" => Ok(ctx.value_of(&NodeId::GlyphAdvance(glyph_name.to_string()))),
        "bbox" => match ctx.value_of(&NodeId::GlyphBbox(glyph_name.to_string())) {
            Value::Rect(rect) => Ok(Value::Rect(rect)),
            _ => ctx.fail(
                mg_syntax::trimmed_range(member.syntax()),
                EvalError::GlyphHasNoInk,
            ),
        },
        "name" => Ok(Value::String(glyph_name.to_string())),
        "codepoints" => {
            let glyph = effective_glyph(ctx.hir, ctx.instance, glyph_name);
            Ok(Value::List(
                glyph
                    .codepoints
                    .iter()
                    .map(|&c| Value::Num(c as f64))
                    .collect(),
            ))
        }
        anchor_name => {
            let glyph = effective_glyph(ctx.hir, ctx.instance, glyph_name);
            if let Some(i) = glyph
                .paths
                .iter()
                .position(|p| p.name.as_deref() == Some(anchor_name))
            {
                // A named path, as authored (spec §5.10).
                return Ok(ctx.value_of(&NodeId::PathRealized(glyph_name.to_string(), i)));
            }
            if glyph.anchors.contains_key(anchor_name) {
                Ok(ctx.value_of(&NodeId::Anchor(
                    glyph_name.to_string(),
                    anchor_name.to_string(),
                )))
            } else {
                unreachable!("mg-hir already type-checked this member access")
            }
        }
    }
}

// ---------------------------------------------------------------------
// Path and glyph geometry

fn eval_path_realized(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    path_index: usize,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let path = &glyph.paths[path_index];

    let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
    let start_seg = &path.segments[0];
    debug_assert_eq!(start_seg.kind, SegmentKind::Start);
    let start_point = eval_expr(&mut ctx, expr_of(&start_seg.at))?
        .as_pair()
        .expect("mg-hir already type-checked `at` as `pair`");

    let mut raw_segments = Vec::with_capacity(path.segments.len().saturating_sub(1));
    for seg in &path.segments[1..] {
        let to = eval_expr(&mut ctx, expr_of(&seg.to))?
            .as_pair()
            .expect("mg-hir already type-checked `to` as `pair`");
        let raw = match seg.kind {
            SegmentKind::Line => mg_geom::skeleton::RawSegment::Line { to },
            SegmentKind::Quad => {
                let c = match &seg.c {
                    Some(e) => Some(eval_pair(&mut ctx, e, "`c`")?),
                    None => None,
                };
                mg_geom::skeleton::RawSegment::Quad { to, c }
            }
            SegmentKind::Cube => {
                let c1 = match &seg.c1 {
                    Some(e) => Some(eval_pair(&mut ctx, e, "`c1`")?),
                    None => None,
                };
                let c2 = eval_pair(&mut ctx, expr_of(&seg.c2), "`c2`")?;
                mg_geom::skeleton::RawSegment::Cube { to, c1, c2 }
            }
            SegmentKind::Arc => {
                let geometry = match &seg.center {
                    Some(center) => mg_geom::skeleton::ArcGeometry::Center(eval_pair(
                        &mut ctx, center, "`center`",
                    )?),
                    None => {
                        let rx = value_num(&mut ctx, expr_of(&seg.rx))?;
                        let ry = value_num(&mut ctx, expr_of(&seg.ry))?;
                        mg_geom::skeleton::ArcGeometry::Radii {
                            rx,
                            ry,
                            large: seg.large,
                        }
                    }
                };
                let sweep = match seg.sweep.expect("mg-hir already validated `sweep`") {
                    Sweep::Ccw => mg_geom::skeleton::Sweep::Ccw,
                    Sweep::Cw => mg_geom::skeleton::Sweep::Cw,
                };
                mg_geom::skeleton::RawSegment::Arc {
                    to,
                    geometry,
                    sweep,
                }
            }
            SegmentKind::Start => {
                unreachable!("a path has exactly one `start`, already consumed")
            }
        };
        raw_segments.push(raw);
    }

    let raw_start = mg_geom::skeleton::RawStart { at: start_point };
    // spec §14: `ARC_TOLERANCE` = `0.01 · font.em / 1000`.
    let arc_tolerance = tolerances(ctx.hir).arc;
    match mg_geom::skeleton::realize(&raw_start, &raw_segments, path.closed, arc_tolerance) {
        Ok(skeleton) => Ok(Value::Path(skeleton)),
        Err(err) => {
            let span = mg_syntax::trimmed_range(&path.syntax);
            ctx.fail(span, err.into())
        }
    }
}

fn eval_pair(ctx: &mut EvalCtx, expr: &ast::Expr, field: &str) -> Result<kurbo::Point, ()> {
    Ok(eval_expr(ctx, expr)?
        .as_pair()
        .unwrap_or_else(|| panic!("mg-hir already type-checked {field} as `pair`")))
}

fn eval_path_bbox(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    path_index: usize,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let path = &glyph.paths[path_index];
    let realized = values[&NodeId::PathRealized(glyph_name.to_string(), path_index)]
        .as_path()
        .expect("PathRealized always evaluates to a Value::Path");

    if !path.renders() {
        return Ok(Value::Rect(construct::bbox(&realized.path)));
    }

    let contours = render_path(hir, instance, glyph_name, path_index, values, diagnostics)?;
    let mut union: Option<kurbo::Rect> = None;
    for (contour, _role) in &contours {
        let bbox = contour.bounding_box();
        union = Some(match union {
            Some(u) => u.union(bbox),
            None => bbox,
        });
    }
    Ok(Value::Rect(
        union
            .expect("`path.renders()` guarantees at least one contour")
            .into(),
    ))
}

/// Spec §14's em-scaled tolerances for this font.
pub(crate) fn tolerances(hir: &Hir) -> mg_geom::tolerance::Tolerances {
    mg_geom::tolerance::Tolerances::for_em(hir.font.em.map_or(0.0, |em| em as f64))
}

/// Every contour a rendering path produces (spec §6.4–§6.5), with its
/// role already assigned (spec §8.1) — except a filled contour's, which
/// only a whole glyph's cross-path nesting count (spec §8.1) can finalize,
/// so it always comes back `Outer` here provisionally. Public so `mg-cli`
/// can call it directly for `mg svg`, the same way it calls
/// [`evaluate`] itself.
///
/// `Err(())` rather than a typed error for the same reason every other
/// `Result<_, ()>` in this module is: the diagnostic already pushed onto
/// `diagnostics` is the real error, not the unit it fails with — the same
/// failure-containment shape (spec §4.6) as [`compute_node`]'s.
#[allow(clippy::result_unit_err)]
pub fn render_path(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    path_index: usize,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<(kurbo::BezPath, mg_geom::winding::ContourRole)>, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let path = &glyph.paths[path_index];
    // Absent rather than a panic-worthy invariant break: called from
    // `compute_node`, `PathRealized` succeeding is already guaranteed by
    // containment (it's one of `PathBbox`'s own dependencies); called
    // directly by `mg-cli` for `mg svg`, it may genuinely have failed —
    // whichever diagnostic that produced is already in `diagnostics` from
    // the `evaluate()` call this came from, so this fails silently.
    let Some(skeleton) = values
        .get(&NodeId::PathRealized(glyph_name.to_string(), path_index))
        .map(|v| {
            v.as_path()
                .expect("PathRealized always evaluates to a Value::Path")
        })
    else {
        return Err(());
    };
    let width = match &path.stroke {
        Some(stroke) => {
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            Some(value_num(&mut ctx, stroke)?)
        }
        None => None,
    };
    let drawing = Drawing {
        skeleton,
        closed: path.closed,
        fill: path.fill,
        width,
        caps: path.caps.as_ref(),
        joins: &path.joins,
        join_at: &path.join_at,
        drawn: drawn_segments(path),
        span: mg_syntax::trimmed_range(&path.syntax),
    };
    render_drawing(hir, &drawing, diagnostics)
}

/// A rendering path's contours, each with its role.
type Contours = Vec<(kurbo::BezPath, mg_geom::winding::ContourRole)>;

/// One thing to draw (spec §6.2): a skeleton and the settings it draws
/// with — a path's own, or a path component's effective ones (spec §5.7).
struct Drawing<'a> {
    skeleton: &'a mg_geom::skeleton::Skeleton,
    closed: bool,
    fill: bool,
    /// The stroke width, when it strokes.
    width: Option<f64>,
    caps: Option<&'a mg_hir::model::CapsSpec>,
    joins: &'a str,
    join_at: &'a IndexMap<String, String>,
    /// The drawn segments `joinAt` keys and stroke errors name; empty when
    /// the skeleton has no declared segments (a `subpath`).
    drawn: &'a [SegmentDecl],
    /// Where an error with no segment of its own points.
    span: Range<usize>,
}

fn render_drawing(
    hir: &Hir,
    drawing: &Drawing,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Contours, ()> {
    let mut contours = Vec::new();

    if drawing.fill {
        // spec §8.3: a self-intersecting filled contour is a hard error,
        // checked before it ever reaches a role or a stroke.
        const INTERSECTION_ACCURACY: f64 = 1e-6;
        if let Err(hit) =
            mg_geom::fill::check_self_intersection(drawing.skeleton, INTERSECTION_ACCURACY)
        {
            diagnostics.push(diagnostic_for(
                drawing.span.clone(),
                EvalError::SelfIntersectingFill {
                    crossings: hit.crossings,
                },
            ));
            return Err(());
        }
        contours.push((
            mg_geom::fill::fill_contour(drawing.skeleton),
            mg_geom::winding::ContourRole::Outer,
        ));
    }

    if let Some(width) = drawing.width {
        let spec = build_stroke_spec(drawing.caps, drawing.joins, drawing.join_at, drawing.drawn, width);
        let offset_tolerance = tolerances(hir).offset;
        match mg_geom::stroke::stroke_path(drawing.skeleton, drawing.closed, &spec, offset_tolerance)
        {
            Ok(mut stroke_contours) => contours.append(&mut stroke_contours),
            Err(err) => {
                let (span, eval_err) =
                    stroke_error_to_eval(drawing.span.clone(), drawing.drawn, err);
                diagnostics.push(diagnostic_for(span, eval_err));
                return Err(());
            }
        }
    }

    Ok(contours)
}

/// Every contour a path component draws (spec §5.7): its transformed
/// skeleton (`ComponentPath`), drawn with the component's own `stroke`,
/// `fill`, `caps`, `joins`, and `joinAt` over its source path's. A
/// computed source (`subpath`, `reverse`, a `let`) has no settings of its
/// own and is open.
#[allow(clippy::result_unit_err)]
pub fn render_path_component(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    index: usize,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Contours, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let component = &glyph.components[index];
    let Some(skeleton) = values
        .get(&NodeId::ComponentPath(glyph_name.to_string(), index))
        .map(|v| {
            v.as_path()
                .expect("ComponentPath always evaluates to a Value::Path")
        })
    else {
        return Err(());
    };
    let source = component
        .source_path(glyph_name)
        .and_then(|(source_glyph, source_path)| {
            let decl = graph::effective_glyph(hir, instance, &source_glyph)?;
            decl.path_named(&source_path).map(|path| (source_glyph, path))
        });

    let width = if let Some(stroke) = &component.stroke {
        let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
        Some(value_num(&mut ctx, stroke)?)
    } else if let Some((source_glyph, path)) = &source
        && let Some(stroke) = &path.stroke
    {
        let mut ctx = EvalCtx::new(hir, instance, Some(source_glyph), values, diagnostics);
        Some(value_num(&mut ctx, stroke)?)
    } else {
        None
    };
    let fill = component
        .fill
        .unwrap_or_else(|| source.as_ref().is_some_and(|(_, p)| p.fill));
    let closed = source.as_ref().is_some_and(|(_, p)| p.closed);
    let span = mg_syntax::trimmed_range(&component.syntax);
    if width.is_none() && !fill {
        diagnostics.push(diagnostic_for(span, EvalError::ComponentDrawsNothing));
        return Err(());
    }
    if fill && !closed {
        diagnostics.push(diagnostic_for(span, EvalError::ComponentFillOnOpenPath));
        return Err(());
    }

    let no_join_at = IndexMap::new();
    let drawing = Drawing {
        skeleton,
        closed,
        fill,
        width,
        caps: component
            .caps
            .as_ref()
            .or_else(|| source.as_ref().and_then(|(_, p)| p.caps.as_ref())),
        joins: component
            .joins
            .as_deref()
            .unwrap_or_else(|| source.as_ref().map_or("miter", |(_, p)| p.joins.as_str())),
        join_at: component
            .join_at
            .as_ref()
            .or_else(|| source.as_ref().map(|(_, p)| &p.join_at))
            .unwrap_or(&no_join_at),
        drawn: source.as_ref().map_or(&[][..], |(_, p)| drawn_segments(p)),
        span,
    };
    render_drawing(hir, &drawing, diagnostics)
}

/// The segments a path draws, `start` excluded. `joinAt` keys and stroke
/// errors index into these.
fn drawn_segments(path: &mg_hir::model::PathDecl) -> &[SegmentDecl] {
    path.segments.get(1..).unwrap_or(&[])
}

/// Builds `mg-geom`'s stroke configuration from a `PathDecl`'s already
/// HIR-validated fields (spec §5.7). `joinAt`'s segment names are
/// resolved to 0-based drawn-segment indices here, since name lookup is
/// this crate's business, not the pure-geometry one's.
fn build_stroke_spec(
    caps: Option<&mg_hir::model::CapsSpec>,
    joins: &str,
    join_at: &IndexMap<String, String>,
    drawn: &[SegmentDecl],
    width: f64,
) -> mg_geom::stroke::StrokeSpec {
    let (start_cap, end_cap) = match caps {
        Some(caps) => (parse_cap(&caps.start), parse_cap(&caps.end)),
        None => (mg_geom::stroke::Cap::Butt, mg_geom::stroke::Cap::Butt),
    };
    let join_overrides = join_at
        .iter()
        .filter_map(|(name, kind)| {
            drawn
                .iter()
                .position(|seg| seg.name.as_deref() == Some(name.as_str()))
                .map(|index| (index, parse_join(kind)))
        })
        .collect();
    mg_geom::stroke::StrokeSpec {
        width,
        start_cap,
        end_cap,
        default_join: parse_join(joins),
        join_overrides,
    }
}

fn parse_cap(s: &str) -> mg_geom::stroke::Cap {
    match s {
        "butt" => mg_geom::stroke::Cap::Butt,
        "round" => mg_geom::stroke::Cap::Round,
        "square" => mg_geom::stroke::Cap::Square,
        _ => unreachable!("mg-hir already validated `caps`"),
    }
}

fn parse_join(s: &str) -> mg_geom::stroke::JoinKind {
    match s {
        "miter" => mg_geom::stroke::JoinKind::Miter,
        "round" => mg_geom::stroke::JoinKind::Round,
        "bevel" => mg_geom::stroke::JoinKind::Bevel,
        _ => unreachable!("mg-hir already validated `joins`/`joinAt`"),
    }
}

/// Where a stroke-stage error's diagnostic should point: a curvature
/// violation or an unresolved inner corner names its own segment (spec
/// §7.2/§7.4), everything else the whole path.
fn stroke_error_to_eval(
    span: Range<usize>,
    drawn: &[SegmentDecl],
    err: mg_geom::stroke::StrokeError,
) -> (Range<usize>, EvalError) {
    // A drawn segment's own span, or the whole drawing's if it has none.
    let span_of = |index: usize| {
        drawn.get(index).map_or_else(
            || span.clone(),
            |segment| mg_syntax::trimmed_range(&segment.syntax),
        )
    };
    match err {
        mg_geom::stroke::StrokeError::ZeroLengthPath => (span.clone(), EvalError::ZeroLengthPath),
        mg_geom::stroke::StrokeError::NonPositiveStroke => {
            (span.clone(), EvalError::NonPositiveStroke)
        }
        mg_geom::stroke::StrokeError::Cusp(violation) => (
            span_of(violation.segment_index),
            EvalError::InteriorCusp {
                segment_index: violation.segment_index,
                local_t: violation.local_t,
            },
        ),
    }
}

/// A component's placement (spec §5.8): `transform` verbatim, `offset` as
/// a pure translation, or the identity when neither is given.
pub(crate) fn component_affine(ctx: &mut EvalCtx, component: &ComponentDecl) -> Result<Affine, ()> {
    if let Some(transform) = &component.transform {
        Ok(eval_expr(ctx, transform)?
            .as_transform()
            .expect("mg-hir already type-checked `transform`"))
    } else if let Some(offset) = &component.offset {
        Ok(Affine::translate(
            eval_expr(ctx, offset)?
                .as_pair()
                .expect("mg-hir already type-checked `offset`")
                .to_vec2(),
        ))
    } else {
        Ok(Affine::IDENTITY)
    }
}

fn eval_glyph_bbox(
    hir: &Hir,
    instance: &InstanceDecl,
    glyph_name: &str,
    values: &IndexMap<NodeId, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Value, ()> {
    let glyph = effective_glyph(hir, instance, glyph_name);
    let mut union: Option<Rect> = None;

    for (i, path) in glyph.paths.iter().enumerate() {
        if !path.renders() {
            continue;
        }
        let rect = values[&NodeId::PathBbox(glyph_name.to_string(), i)]
            .as_rect()
            .expect("PathBbox always evaluates to a Value::Rect");
        union = Some(union.map_or(rect, |u| union_rect(u, rect)));
    }

    let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
    for (i, component) in glyph.components.iter().enumerate() {
        if component.path.is_some() {
            // Already in this glyph's authored coordinates (spec §10.1).
            if let Some(rect) = ctx
                .values
                .get(&NodeId::ComponentBbox(glyph_name.to_string(), i))
                .and_then(Value::as_rect)
            {
                union = Some(union.map_or(rect, |u| union_rect(u, rect)));
            }
            continue;
        }
        let Some(target) = &component.glyph else {
            continue;
        };
        let target_bbox = ctx
            .values
            .get(&NodeId::GlyphBbox(target.clone()))
            .expect("a component's target glyph bbox is a dependency");
        // An inkless component adds nothing.
        let Some(target_rect) = target_bbox.as_rect() else {
            continue;
        };
        // The target is drawn as authored, without its own shift (spec
        // §10.1).

        let affine = component_affine(&mut ctx, component)?;

        let transformed = transform_rect(target_rect, affine);
        union = Some(union.map_or(transformed, |u| union_rect(u, transformed)));
    }

    Ok(union.map_or(Value::NoInk, Value::Rect))
}

/// The glyph's ink in authored coordinates, for spacing derived from it
/// (spec §12.1). A glyph with no ink has no sidebearings; the failure is
/// reported on `bearing`, the field that asked for them.
fn spacing_ink(ctx: &mut EvalCtx, glyph_name: &str, bearing: &ast::Expr) -> Result<Rect, ()> {
    match ctx.value_of(&NodeId::GlyphBbox(glyph_name.to_string())) {
        Value::Rect(rect) => Ok(rect),
        _ => ctx
            .fail(
                mg_syntax::trimmed_range(bearing.syntax()),
                EvalError::GlyphHasNoInk,
            )
            .map(|_| unreachable!("`fail` always returns `Err`")),
    }
}

/// The advance (spec §12.1): declared, or derived from the bearings and
/// the ink width. `rsb` defaults to `lsb`.
fn eval_glyph_advance(ctx: &mut EvalCtx, glyph_name: &str) -> Result<f64, ()> {
    let glyph = effective_glyph(ctx.hir, ctx.instance, glyph_name);
    if let Some(advance) = &glyph.advance {
        return value_num(ctx, advance);
    }
    match (&glyph.lsb, &glyph.rsb) {
        (Some(lsb), rsb) => {
            let ink = spacing_ink(ctx, glyph_name, lsb)?;
            let left = value_num(ctx, lsb)?;
            let right = match rsb {
                Some(rsb) => value_num(ctx, rsb)?,
                None => left,
            };
            Ok(left + ink.width() + right)
        }
        (None, Some(rsb)) => {
            let ink = spacing_ink(ctx, glyph_name, rsb)?;
            Ok(ink.x1 + value_num(ctx, rsb)?)
        }
        (None, None) => unreachable!("mg-hir requires one of `advance`, `lsb`, `rsb`"),
    }
}

/// The horizontal shift from authored to placed coordinates (spec §12.1):
/// whatever puts the ink's left edge at `lsb`, or its right edge `rsb`
/// short of a declared `advance`; otherwise zero.
fn eval_glyph_shift(ctx: &mut EvalCtx, glyph_name: &str) -> Result<f64, ()> {
    let glyph = effective_glyph(ctx.hir, ctx.instance, glyph_name);
    if let Some(lsb) = &glyph.lsb {
        let ink = spacing_ink(ctx, glyph_name, lsb)?;
        return Ok(value_num(ctx, lsb)? - ink.x0);
    }
    if let (Some(rsb), Some(_)) = (&glyph.rsb, &glyph.advance) {
        let ink = spacing_ink(ctx, glyph_name, rsb)?;
        let advance = ctx
            .value_of(&NodeId::GlyphAdvance(glyph_name.to_string()))
            .as_num()
            .expect("GlyphAdvance always evaluates to a Value::Num");
        return Ok(advance - value_num(ctx, rsb)? - ink.x1);
    }
    Ok(0.0)
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

fn transform_rect(r: Rect, affine: Affine) -> Rect {
    let corners = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    let mut path = kurbo::BezPath::new();
    path.move_to(affine * corners[0]);
    for c in &corners[1..] {
        path.line_to(affine * *c);
    }
    path.close_path();
    path.bounding_box().into()
}

// ---------------------------------------------------------------------
// Diagnostics

fn diagnostic_for(span: Range<usize>, err: EvalError) -> Diagnostic {
    let code = match &err {
        EvalError::DivisionByZero => codes::DIVISION_BY_ZERO,
        EvalError::SqrtOfNegative(_) | EvalError::LogOfNonPositive(_) => codes::SQRT_OF_NEGATIVE,
        EvalError::InverseTrigOutOfRange { .. } => codes::INVERSE_TRIG_OUT_OF_RANGE,
        EvalError::PowerDomainError { .. } => codes::POWER_DOMAIN_ERROR,
        EvalError::MeetOnParallelLines => codes::MEET_ON_PARALLEL_LINES,
        EvalError::ComponentDrawsNothing => codes::COMPONENT_DRAWS_NOTHING,
        EvalError::ComponentFillOnOpenPath => codes::FILL_REQUIRES_CLOSED_PATH,
        EvalError::LineThroughOnePoint => codes::LINE_THROUGH_ONE_POINT,
        EvalError::NonPositiveRadius(_) => codes::NON_POSITIVE_RADIUS,
        EvalError::CastMissesEllipse => codes::CAST_MISSES_ELLIPSE,
        EvalError::UnitOfZeroVector => codes::ZERO_VECTOR,
        EvalError::PathParameterOutOfDomain { .. } => codes::PATH_PARAMETER_OUT_OF_DOMAIN,
        EvalError::EmptyListReduction { .. } => codes::EMPTY_LIST_REDUCTION,
        EvalError::GlyphHasNoInk => codes::GLYPH_HAS_NO_INK,
        EvalError::NoAxisAlignedEllipse => codes::NO_AXIS_ALIGNED_ELLIPSE,
        EvalError::RadiiTooSmallForChord => codes::RADII_TOO_SMALL_FOR_CHORD,
        EvalError::ZeroLengthSegment => codes::ZERO_LENGTH_SEGMENT,
        EvalError::ZeroLengthPath => codes::ZERO_LENGTH_PATH,
        EvalError::NonPositiveStroke => codes::NON_POSITIVE_STROKE,
        EvalError::InteriorCusp { .. } => codes::INTERIOR_CUSP,
        EvalError::SelfIntersectingFill { .. } => codes::SELF_INTERSECTING_FILL,
    };
    let diagnostic = Diagnostic::error(code, err.to_string(), Label::new(span, "here"));
    match err {
        EvalError::CastMissesEllipse => diagnostic.with_help(
            "`crossings(l, e)` lists every crossing, including those behind the origin",
        ),
        EvalError::NoAxisAlignedEllipse => diagnostic.with_help(
            "or switch to radii mode: a half-oval from the top of an oval to its bottom is \
             `arc (to: b, rx: w/2, ry: h/2, sweep: \"cw\")`",
        ),
        _ => diagnostic,
    }
}

fn cycle_diagnostic(hir: &Hir, cycle: &[NodeId]) -> Diagnostic {
    let path = cycle
        .iter()
        .map(NodeId::to_string)
        .collect::<Vec<_>>()
        .join(" → ");
    let first_span = node_span(hir, &cycle[0]);
    let mut diagnostic = Diagnostic::error(
        codes::CYCLE,
        format!("circular definition: {path} → {}", cycle[0]),
        Label::new(first_span, "part of this cycle"),
    );
    for node in &cycle[1..] {
        diagnostic = diagnostic.with_secondary(Label::new(
            node_span(hir, node),
            format!("`{node}` declared here"),
        ));
    }
    diagnostic.with_help(format!(
        "break the cycle by deriving one of {path} from a `param` or a literal instead"
    ))
}

fn node_span(hir: &Hir, node: &NodeId) -> Range<usize> {
    let syntax = match node {
        NodeId::TopLevel(name) => {
            if let Some(p) = hir.params.get(name) {
                &p.syntax
            } else if let Some(m) = hir.metrics.get(name) {
                &m.syntax
            } else {
                &hir.lets[name].syntax
            }
        }
        NodeId::GlyphLocal(glyph, name) => &hir.glyphs[&(glyph.clone(), None)].lets[name].syntax,
        NodeId::GlyphAdvance(glyph) | NodeId::GlyphShift(glyph) | NodeId::GlyphBbox(glyph) => {
            &hir.glyphs[&(glyph.clone(), None)].syntax
        }
        NodeId::PathRealized(glyph, i) | NodeId::PathBbox(glyph, i) => {
            &hir.glyphs[&(glyph.clone(), None)].paths[*i].syntax
        }
        NodeId::ComponentPath(glyph, i) | NodeId::ComponentBbox(glyph, i) => {
            &hir.glyphs[&(glyph.clone(), None)].components[*i].syntax
        }
        NodeId::Anchor(glyph, name) => &hir.glyphs[&(glyph.clone(), None)].anchors[name].syntax,
        NodeId::Kern(i) => &hir.kerns[*i].syntax,
    };
    mg_syntax::trimmed_range(syntax)
}

// ---------------------------------------------------------------------
// Incremental re-evaluation (spec §4.5)

/// `changed` plus every node reachable by following dependency edges
/// forward from it — the subgraph an edit to `changed` can possibly
/// affect.
pub fn dirty_closure(graph: &Graph, changed: &NodeId) -> IndexSet<NodeId> {
    let mut successors: IndexMap<&NodeId, Vec<&NodeId>> = IndexMap::new();
    for node in &graph.nodes {
        for dep in &graph.deps[node] {
            successors.entry(dep).or_default().push(node);
        }
    }

    let mut dirty = IndexSet::new();
    dirty.insert(changed.clone());
    let mut queue = vec![changed.clone()];
    while let Some(node) = queue.pop() {
        for &succ in successors.get(&node).into_iter().flatten() {
            if dirty.insert(succ.clone()) {
                queue.push(succ.clone());
            }
        }
    }
    dirty
}

/// Re-evaluates only `changed`'s dirty closure, reusing every other
/// cached value and diagnostic from `previous` untouched (spec §4.5: "An
/// edit dirties the node it touches and everything downstream.
/// Re-evaluate that subgraph only").
pub fn reevaluate(
    hir: &Hir,
    instance: &InstanceDecl,
    graph: &Graph,
    previous: &EvalOutcome,
    changed: &NodeId,
) -> EvalOutcome {
    let dirty = dirty_closure(graph, changed);
    let TopoOutcome { sorted, remaining } = toposort::topo_sort(graph);

    let mut values = previous.values.clone();
    let mut failed = previous.failed.clone();
    for node in &dirty {
        values.shift_remove(node);
        failed.shift_remove(node);
    }

    let mut diagnostics = Vec::new();
    if remaining.iter().any(|n| dirty.contains(n)) {
        let cycle = toposort::find_cycle(graph, &remaining);
        diagnostics.push(cycle_diagnostic(hir, &cycle));
        for node in remaining.iter().filter(|n| dirty.contains(*n)) {
            failed.insert(node.clone());
        }
    }

    for node in sorted.iter().filter(|n| dirty.contains(*n)) {
        eval_node(
            hir,
            instance,
            graph,
            node,
            &mut values,
            &mut failed,
            &mut diagnostics,
        );
    }

    EvalOutcome {
        values,
        failed,
        diagnostics,
    }
}

trait Vec2Mul {
    fn mul(self, s: f64) -> Self;
}

impl Vec2Mul for kurbo::Vec2 {
    fn mul(self, s: f64) -> Self {
        kurbo::Vec2::new(self.x * s, self.y * s)
    }
}
