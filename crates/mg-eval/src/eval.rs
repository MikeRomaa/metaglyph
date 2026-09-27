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
use mg_hir::model::{Align, GlyphDecl, Hir, InstanceDecl, MetricDecl, ParamDecl, SegmentKind};
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
    let outcome = run(hir, instance, &graph);
    (graph, outcome)
}

fn run(hir: &Hir, instance: &InstanceDecl, graph: &Graph) -> EvalOutcome {
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
            let glyph = effective_glyph(hir, instance, glyph_name);
            let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
            eval_expr(&mut ctx, expr_of(&glyph.advance))
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
    }
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

struct EvalCtx<'a> {
    hir: &'a Hir,
    instance: &'a InstanceDecl,
    current_glyph: Option<&'a str>,
    values: &'a IndexMap<NodeId, Value>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> EvalCtx<'a> {
    fn new(
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
            diagnostics,
        }
    }

    fn fail(&mut self, span: Range<usize>, err: EvalError) -> Result<Value, ()> {
        self.diagnostics.push(diagnostic_for(span, err));
        Err(())
    }

    fn value_of(&self, node: &NodeId) -> Value {
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
    match construct::call(&name, &values) {
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
                return eval_glyph_member(ctx, &glyph_name, field);
            }
            _ => {}
        }
    }

    if let ast::Expr::Member(inner) = &receiver
        && let Some(ast::Expr::Ident(root)) = inner.receiver()
        && root.token().as_ref().map(|t| t.text()) == Some("glyphs")
        && let Some(glyph_name_token) = inner.member_token()
    {
        return eval_glyph_member(ctx, glyph_name_token.text(), field);
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
            "bbox" => Value::Rect(construct::bbox(&p)),
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

fn eval_glyph_member(ctx: &mut EvalCtx, glyph_name: &str, field: &str) -> Result<Value, ()> {
    match field {
        "advance" => Ok(ctx.value_of(&NodeId::GlyphAdvance(glyph_name.to_string()))),
        "bbox" => Ok(ctx.value_of(&NodeId::GlyphBbox(glyph_name.to_string()))),
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

    if let Some(target_name) = &path.follows {
        // `follows` takes the target's skeleton exactly (spec §5.7); the
        // target is itself a dependency (see `crate::graph`), so its
        // value is already in the cache.
        let target_index = glyph
            .paths
            .iter()
            .position(|p| p.name.as_deref() == Some(target_name.as_str()))
            .expect("mg-hir already resolved `follows`");
        return Ok(values[&NodeId::PathRealized(glyph_name.to_string(), target_index)].clone());
    }

    let mut ctx = EvalCtx::new(hir, instance, Some(glyph_name), values, diagnostics);
    let start_seg = &path.segments[0];
    debug_assert_eq!(start_seg.kind, SegmentKind::Start);
    let start_point = eval_expr(&mut ctx, expr_of(&start_seg.at))?
        .as_pair()
        .expect("mg-hir already type-checked `at` as `pair`");
    let start_dir = match &start_seg.dir {
        Some(e) => Some(eval_direction(&mut ctx, e)?),
        None => None,
    };

    let mut raw_segments = Vec::with_capacity(path.segments.len().saturating_sub(1));
    for seg in &path.segments[1..] {
        let to = eval_expr(&mut ctx, expr_of(&seg.to))?
            .as_pair()
            .expect("mg-hir already type-checked `to` as `pair`");
        let dir = match &seg.dir {
            Some(e) => Some(eval_direction(&mut ctx, e)?),
            None => None,
        };
        let from_dir = match &seg.from_dir {
            Some(e) => Some(eval_direction(&mut ctx, e)?),
            None => None,
        };
        let tension = eval_tension(&mut ctx, &seg.tension)?;
        let controls = match &seg.controls {
            Some((c0, c1)) => {
                let p0 = eval_expr(&mut ctx, c0)?
                    .as_pair()
                    .expect("mg-hir already type-checked `controls`");
                let p1 = eval_expr(&mut ctx, c1)?
                    .as_pair()
                    .expect("mg-hir already type-checked `controls`");
                Some((p0, p1))
            }
            None => None,
        };
        raw_segments.push(mg_geom::skeleton::RawSegment {
            kind: match seg.kind {
                SegmentKind::Line => mg_geom::skeleton::SegmentKind::Line,
                SegmentKind::Spline => mg_geom::skeleton::SegmentKind::Spline,
                SegmentKind::Start => {
                    unreachable!("a path has exactly one `start`, already consumed")
                }
            },
            to,
            dir,
            from_dir,
            tension,
            controls,
        });
    }

    let raw_start = mg_geom::skeleton::RawStart {
        at: start_point,
        dir: start_dir,
    };
    match mg_geom::skeleton::realize(&raw_start, &raw_segments, path.closed) {
        Ok(bez) => Ok(Value::Path(bez)),
        Err(err) => {
            let span = mg_syntax::trimmed_range(&path.syntax);
            ctx.fail(span, err.into())
        }
    }
}

fn eval_direction(ctx: &mut EvalCtx, expr: &ast::Expr) -> Result<kurbo::Vec2, ()> {
    Ok(eval_expr(ctx, expr)?
        .as_pair()
        .expect("mg-hir already type-checked this as `pair`")
        .to_vec2())
}

fn eval_tension(ctx: &mut EvalCtx, expr: &Option<ast::Expr>) -> Result<(f64, f64), ()> {
    let Some(expr) = expr else {
        return Ok((1.0, 1.0));
    };
    match eval_expr(ctx, expr)? {
        Value::Num(n) => Ok((n, n)),
        Value::Pair(p) => Ok((p.x, p.y)),
        _ => unreachable!("mg-hir already type-checked `tension` as `num` or `pair`"),
    }
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
    if path.renders() {
        let span = mg_syntax::trimmed_range(&path.syntax);
        diagnostics.push(diagnostic_for(span, EvalError::StrokingNotYetImplemented));
        return Err(());
    }
    let realized = values[&NodeId::PathRealized(glyph_name.to_string(), path_index)]
        .as_path()
        .expect("PathRealized always evaluates to a Value::Path");
    Ok(Value::Rect(construct::bbox(realized)))
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
    for component in &glyph.components {
        let Some(target) = &component.glyph else {
            continue;
        };
        let target_rect = ctx
            .values
            .get(&NodeId::GlyphBbox(target.clone()))
            .expect("a component's target glyph bbox is a dependency")
            .as_rect()
            .expect("GlyphBbox always evaluates to a Value::Rect");

        let affine = if let Some(transform) = &component.transform {
            eval_expr(&mut ctx, transform)?
                .as_transform()
                .expect("mg-hir already type-checked `transform`")
        } else if let Some(offset) = &component.offset {
            Affine::translate(
                eval_expr(&mut ctx, offset)?
                    .as_pair()
                    .expect("mg-hir already type-checked `offset`")
                    .to_vec2(),
            )
        } else {
            Affine::IDENTITY
        };

        let transformed = transform_rect(target_rect, affine);
        union = Some(union.map_or(transformed, |u| union_rect(u, transformed)));
    }

    match union {
        Some(rect) => Ok(Value::Rect(rect)),
        None => {
            let span = mg_syntax::trimmed_range(&glyph.syntax);
            ctx.fail(span, EvalError::GlyphHasNoInk)
        }
    }
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
        EvalError::UnitOfZeroVector => codes::ZERO_VECTOR,
        EvalError::PathParameterOutOfDomain { .. } => codes::PATH_PARAMETER_OUT_OF_DOMAIN,
        EvalError::EmptyListReduction { .. } => codes::EMPTY_LIST_REDUCTION,
        EvalError::GlyphHasNoInk => codes::GLYPH_HAS_NO_INK,
        EvalError::FreeDirection => codes::FREE_DIRECTION_NOT_YET_IMPLEMENTED,
        EvalError::StrokingNotYetImplemented => codes::STROKING_NOT_YET_IMPLEMENTED,
        EvalError::ZeroLengthSegment => codes::ZERO_LENGTH_SEGMENT,
        EvalError::NeedsBezierClipping => codes::NEEDS_BEZIER_CLIPPING,
    };
    Diagnostic::error(code, err.to_string(), Label::new(span, "here"))
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
        NodeId::GlyphAdvance(glyph) | NodeId::GlyphBbox(glyph) => {
            &hir.glyphs[&(glyph.clone(), None)].syntax
        }
        NodeId::PathRealized(glyph, i) | NodeId::PathBbox(glyph, i) => {
            &hir.glyphs[&(glyph.clone(), None)].paths[*i].syntax
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
