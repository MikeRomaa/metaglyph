//! Read-only views of a [`Model`] for one instance (plan 6, W2): the
//! font-wide data every sheet shares ([`font_data`]) and one glyph's
//! drawing ([`glyph_scene`]). Coordinates are font units, y-up. Source
//! spans are UTF-16 `[from, to]` pairs.

use std::ops::Range;

use indexmap::IndexMap;
use kurbo::{Affine, PathEl, Point};
use mg_diag::Severity;
use mg_eval::NodeId;
use mg_eval::graph::effective_glyph;
use mg_eval::value::Value;
use mg_hir::model::{GlyphDecl, InstanceDecl, KernSide, SegmentKind};
use mg_syntax::SyntaxNode;
use mg_syntax::ast::{self, AstNode};
use serde::Serialize;

use crate::doc::Model;
use crate::offsets::Utf16Index;

type Span = [usize; 2];
type Pt = [f64; 2];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontData {
    pub instance: String,
    pub em: f64,
    pub metrics: Vec<MetricInfo>,
    pub lets: Vec<LetInfo>,
    pub glyphs: Vec<GlyphInfo>,
    pub groups: Vec<GroupInfo>,
    pub kerns: Vec<KernInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricInfo {
    pub name: String,
    pub y: Option<f64>,
    pub overshoot: Option<f64>,
    pub expr: String,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LetInfo {
    pub name: String,
    pub expr: String,
    /// The value as the inspector prints it.
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphInfo {
    pub name: String,
    pub codepoints: Vec<u32>,
    pub span: Span,
    pub advance: Option<f64>,
    /// Authored → placed x offset (spec §12.1).
    pub shift: Option<f64>,
    /// Ink bounds in authored coordinates, `[x0, y0, x1, y1]`; `None` for
    /// a glyph with no ink.
    pub ink: Option<[f64; 4]>,
    /// The declared spacing fields' source text.
    pub fields: SpacingFields,
    /// Every contour, components decomposed, in placed coordinates, as one
    /// SVG path `d`.
    pub outline: String,
    pub components: usize,
    pub errors: usize,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpacingFields {
    pub advance: Option<String>,
    pub lsb: Option<String>,
    pub rsb: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub name: String,
    pub glyphs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernInfo {
    pub left: Option<String>,
    pub left_group: bool,
    pub right: Option<String>,
    pub right_group: bool,
    pub by: Option<f64>,
    pub expr: String,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphScene {
    pub name: String,
    pub span: Span,
    /// The glyph's own contours, in authored coordinates.
    pub outline: Vec<String>,
    pub components: Vec<ComponentInfo>,
    pub paths: Vec<PathInfo>,
    pub points: Vec<PointInfo>,
    pub lines: Vec<LineInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub glyph: String,
    /// Decomposed, in this glyph's authored coordinates.
    pub outline: String,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub index: usize,
    pub name: Option<String>,
    pub span: Span,
    pub stroke: Option<String>,
    pub caps: Option<[String; 2]>,
    pub joins: String,
    pub fill: bool,
    pub enabled: bool,
    pub closed: bool,
    pub follows: Option<String>,
    /// The realized skeleton; `None` when it failed to evaluate.
    pub skeleton: Option<String>,
    /// A point on the skeleton for the path's callout to touch: halfway
    /// along its first segment.
    pub anchor: Option<Pt>,
    pub segments: Vec<SegmentInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentInfo {
    pub kind: &'static str,
    pub name: Option<String>,
    pub span: Span,
    /// The on-curve end point (`start`: its `at`).
    pub to: Option<Pt>,
    /// The local `let` `to`/`at` names, when it is a bare name.
    pub to_ref: Option<String>,
    /// Off-curve control points, in order.
    pub controls: Vec<Pt>,
    /// An `arc`'s ellipse; `None` for other kinds or when it failed.
    pub arc: Option<ArcInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcInfo {
    pub center: Pt,
    pub rx: f64,
    pub ry: f64,
    /// Where the arc starts: the previous segment's end.
    pub from: Pt,
    /// Radii mode's `rx`/`ry` source text; `None` in centre mode, where the
    /// radii are solved.
    pub rx_expr: Option<String>,
    pub ry_expr: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointInfo {
    pub name: String,
    pub at: Pt,
    /// `"skeleton"` when a segment names it as its `at`/`to`, otherwise
    /// `"construction"`.
    pub role: &'static str,
    pub expr: String,
    /// The called function's name when the expression is a call, e.g.
    /// `meet` — the design labels construction points with it.
    pub callee: Option<String>,
    pub span: Span,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineInfo {
    pub name: String,
    pub p0: Pt,
    pub p1: Pt,
    pub expr: String,
    pub span: Span,
    /// For the ray a `polar(q, len, θ)` point is placed along: that
    /// point's name. The line runs from `q` through the point.
    pub of: Option<String>,
    /// For a `polar` ray: the `len` argument's source text.
    pub radius_expr: Option<String>,
}

struct Ctx<'a> {
    model: &'a Model,
    index: Utf16Index,
    instance: &'a InstanceDecl,
    values: &'a IndexMap<NodeId, Value>,
}

impl<'a> Ctx<'a> {
    fn new(model: &'a Model, instance: &str) -> Option<Self> {
        let decl = model.hir.instances.get(instance)?;
        let outcome = model.outcomes.get(instance)?;
        Some(Self {
            model,
            index: Utf16Index::new(&model.source),
            instance: decl,
            values: &outcome.values,
        })
    }

    fn value(&self, node: NodeId) -> Option<&Value> {
        self.values.get(&node)
    }

    fn num(&self, node: NodeId) -> Option<f64> {
        self.value(node).and_then(Value::as_num)
    }

    /// A node's whitespace-trimmed source range.
    fn range(&self, node: &SyntaxNode) -> Range<usize> {
        let full: Range<usize> = node.text_range().into();
        let text = &self.model.source[full.clone()];
        let start = full.start + (text.len() - text.trim_start().len());
        let end = full.end - (text.len() - text.trim_end().len());
        start..end.max(start)
    }

    fn span(&self, node: &SyntaxNode) -> Span {
        self.index.span(&self.range(node))
    }

    fn text(&self, node: &SyntaxNode) -> String {
        self.model.source[self.range(node)].to_string()
    }

    fn expr_text(&self, expr: Option<&ast::Expr>) -> String {
        expr.map(|e| self.text(e.syntax())).unwrap_or_default()
    }
}

/// The instance-wide data for `instance`, or `None` if there is no such
/// instance.
pub fn font_data(model: &Model, instance: &str) -> Option<FontData> {
    let ctx = Ctx::new(model, instance)?;
    let hir = &model.hir;

    let metrics = hir
        .metrics
        .values()
        .map(|m| {
            let zone = match ctx.value(NodeId::TopLevel(m.name.clone())) {
                Some(Value::Zone(z)) => Some(*z),
                _ => None,
            };
            MetricInfo {
                name: m.name.clone(),
                y: zone.map(|z| z.y),
                overshoot: zone.map(|z| z.overshoot),
                expr: ctx.expr_text(m.y.as_ref()),
                span: ctx.span(&m.syntax),
            }
        })
        .collect();

    let lets = hir
        .lets
        .values()
        .map(|l| LetInfo {
            name: l.name.clone(),
            expr: ctx.expr_text(l.value.as_ref()),
            value: ctx
                .value(NodeId::TopLevel(l.name.clone()))
                .map(format_value)
                .unwrap_or_else(|| "—".to_string()),
            span: ctx.span(&l.syntax),
        })
        .collect();

    let glyphs = hir
        .glyphs
        .keys()
        .filter(|(_, set)| set.is_none())
        .filter_map(|(name, _)| effective_glyph(hir, ctx.instance, name))
        .map(|glyph| glyph_info(&ctx, glyph))
        .collect();

    let groups = hir
        .groups
        .values()
        .map(|g| GroupInfo {
            name: g.name.clone(),
            glyphs: g.glyphs.clone(),
            span: ctx.span(&g.syntax),
        })
        .collect();

    let side = |side: &Option<KernSide>, raw: &Option<String>| match side {
        Some(KernSide::Glyph(name)) => (Some(name.clone()), false),
        Some(KernSide::Group(name)) => (Some(name.clone()), true),
        None => (raw.clone(), false),
    };
    let kerns = hir
        .kerns
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let (left, left_group) = side(&k.left, &k.left_name);
            let (right, right_group) = side(&k.right, &k.right_name);
            KernInfo {
                left,
                left_group,
                right,
                right_group,
                by: ctx.num(NodeId::Kern(i)),
                expr: ctx.expr_text(k.by.as_ref()),
                span: ctx.span(&k.syntax),
            }
        })
        .collect();

    Some(FontData {
        instance: instance.to_string(),
        em: hir.font.em.unwrap_or(1000) as f64,
        metrics,
        lets,
        glyphs,
        groups,
        kerns,
    })
}

fn glyph_info(ctx: &Ctx, glyph: &GlyphDecl) -> GlyphInfo {
    let name = &glyph.name;
    let range = ctx.range(&glyph.syntax);
    let errors = ctx
        .model
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .filter(|d| range.start <= d.primary.span.start && d.primary.span.end <= range.end)
        .count();

    let outcome = &ctx.model.outcomes[&ctx.instance.name];
    let mut ignored = Vec::new();
    let outline = mg_eval::render_glyph(
        &ctx.model.hir,
        ctx.instance,
        name,
        ctx.values,
        &outcome.failed,
        &mut ignored,
    )
    .unwrap_or_default()
    .iter()
    .map(|(path, _)| path.to_svg())
    .collect::<Vec<_>>()
    .join(" ");

    GlyphInfo {
        name: name.clone(),
        codepoints: glyph.codepoints.clone(),
        span: ctx.index.span(&range),
        advance: ctx.num(NodeId::GlyphAdvance(name.clone())),
        shift: ctx.num(NodeId::GlyphShift(name.clone())),
        ink: match ctx.value(NodeId::GlyphBbox(name.clone())) {
            Some(Value::Rect(r)) => Some([r.x0, r.y0, r.x1, r.y1]),
            _ => None,
        },
        fields: SpacingFields {
            advance: glyph.advance.as_ref().map(|e| ctx.text(e.syntax())),
            lsb: glyph.lsb.as_ref().map(|e| ctx.text(e.syntax())),
            rsb: glyph.rsb.as_ref().map(|e| ctx.text(e.syntax())),
        },
        outline,
        components: glyph.components.len(),
        errors,
    }
}

/// One glyph's drawing for `instance`, in authored coordinates, or `None`
/// if there is no such glyph or instance.
pub fn glyph_scene(model: &Model, instance: &str, name: &str) -> Option<GlyphScene> {
    let ctx = Ctx::new(model, instance)?;
    let hir = &model.hir;
    let glyph = effective_glyph(hir, ctx.instance, name)?;
    let outcome = &model.outcomes[instance];
    let shift = ctx.num(NodeId::GlyphShift(name.to_string())).unwrap_or(0.0);
    let unshift = Affine::translate((-shift, 0.0));
    let mut ignored = Vec::new();

    let own = mg_eval::glyph_outline(
        hir,
        ctx.instance,
        name,
        ctx.values,
        &outcome.failed,
        &mut ignored,
    );
    let outline = own
        .contours
        .iter()
        .map(|c| (unshift * c.path.clone()).to_svg())
        .collect();

    // `glyph_outline` skips failed components, so match placed components
    // to declarations by glyph name, in order.
    let mut placed = own.components.iter().peekable();
    let components = glyph
        .components
        .iter()
        .filter_map(|decl| {
            let target = decl.glyph.as_ref()?;
            let component = placed.next_if(|c| &c.glyph == target)?;
            let contours = mg_eval::render_glyph(
                hir,
                ctx.instance,
                target,
                ctx.values,
                &outcome.failed,
                &mut ignored,
            )
            .unwrap_or_default();
            let transform = unshift * component.transform;
            Some(ComponentInfo {
                glyph: target.clone(),
                outline: contours
                    .iter()
                    .map(|(path, _)| (transform * path.clone()).to_svg())
                    .collect::<Vec<_>>()
                    .join(" "),
                span: ctx.span(&decl.syntax),
            })
        })
        .collect();

    let local = |expr: Option<&ast::Expr>| -> Option<String> {
        let ast::Expr::Ident(ident) = expr? else {
            return None;
        };
        let name = ident.token()?.text().to_string();
        glyph.lets.contains_key(&name).then_some(name)
    };

    let mut skeleton_points = std::collections::HashSet::new();
    let paths = glyph
        .paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let skeleton = match ctx.value(NodeId::PathRealized(name.to_string(), index)) {
                Some(Value::Path(s)) => Some(s),
                _ => None,
            };
            let ends = skeleton.map(authored_ends).unwrap_or_default();
            let mut drawn = ends.into_iter();
            let mut prev: Option<Pt> = None;
            let segments = path
                .segments
                .iter()
                .map(|seg| {
                    let end_expr = if seg.kind == SegmentKind::Start {
                        seg.at.as_ref()
                    } else {
                        seg.to.as_ref()
                    };
                    let to_ref = local(end_expr);
                    if let Some(point) = &to_ref {
                        skeleton_points.insert(point.clone());
                    }
                    let (to, controls) = drawn.next().unwrap_or((None, Vec::new()));
                    let arc = match (seg.kind, prev, to) {
                        (SegmentKind::Arc, Some(from), Some(to)) => arc_info(&ctx, name, seg, from, to),
                        _ => None,
                    };
                    prev = to;
                    SegmentInfo {
                        kind: match seg.kind {
                            SegmentKind::Start => "start",
                            SegmentKind::Line => "line",
                            SegmentKind::Quad => "quad",
                            SegmentKind::Cube => "cube",
                            SegmentKind::Arc => "arc",
                        },
                        name: seg.name.clone(),
                        span: ctx.span(&seg.syntax),
                        to,
                        to_ref,
                        controls,
                        arc,
                    }
                })
                .collect();
            PathInfo {
                index,
                name: path.name.clone(),
                span: ctx.span(&path.syntax),
                stroke: path.stroke.as_ref().map(|e| ctx.text(e.syntax())),
                caps: path.caps.as_ref().map(|c| [c.start.clone(), c.end.clone()]),
                joins: path.joins.clone(),
                fill: path.fill,
                enabled: path.enabled,
                closed: path.closed,
                follows: path.follows.clone(),
                skeleton: skeleton.map(|s| s.path.to_svg()),
                anchor: skeleton.and_then(|s| {
                    use kurbo::ParamCurve;
                    let (piece, t) = mg_geom::skeleton::resolve_param(s, 0.5).ok()?;
                    Some(pt(piece.eval(t)))
                }),
                segments,
            }
        })
        .collect();

    let mut points = Vec::new();
    let mut lines = Vec::new();
    for decl in glyph.lets.values() {
        let node = NodeId::GlyphLocal(name.to_string(), decl.name.clone());
        let expr = ctx.expr_text(decl.value.as_ref());
        let span = ctx.span(&decl.syntax);
        match ctx.value(node) {
            Some(Value::Pair(p)) => {
                let call = match &decl.value {
                    Some(ast::Expr::Call(call)) => Some(call),
                    _ => None,
                };
                let callee = call.and_then(|c| c.callee()).map(|c| ctx.text(c.syntax()));
                let args: Vec<ast::Expr> = call
                    .and_then(|c| c.arg_list())
                    .map(|a| a.args().collect())
                    .unwrap_or_default();
                if callee.as_deref() == Some("polar")
                    && let Some(origin) = args.first()
                    && let Some(Value::Pair(q)) =
                        mg_eval::eval_subexpr(hir, ctx.instance, Some(name), ctx.values, origin)
                    && q.distance(*p) > 1e-9
                {
                    lines.push(LineInfo {
                        name: decl.name.clone(),
                        p0: pt(q),
                        p1: pt(*p),
                        expr: expr.clone(),
                        span,
                        of: Some(decl.name.clone()),
                        radius_expr: args.get(1).map(|e| ctx.text(e.syntax())),
                    });
                }
                points.push(PointInfo {
                    name: decl.name.clone(),
                    at: pt(*p),
                    role: if skeleton_points.contains(&decl.name) {
                        "skeleton"
                    } else {
                        "construction"
                    },
                    callee,
                    expr,
                    span,
                });
            }
            Some(Value::Line(l)) => lines.push(LineInfo {
                name: decl.name.clone(),
                p0: pt(l.p0),
                p1: pt(l.p1),
                expr,
                span,
                of: None,
                radius_expr: None,
            }),
            _ => {}
        }
    }

    Some(GlyphScene {
        name: name.to_string(),
        span: ctx.span(&glyph.syntax),
        outline,
        components,
        paths,
        points,
        lines,
    })
}

/// An `arc` segment's ellipse, from its evaluated `center` or `rx`/`ry`.
fn arc_info(
    ctx: &Ctx,
    glyph: &str,
    seg: &mg_hir::model::SegmentDecl,
    from: Pt,
    to: Pt,
) -> Option<ArcInfo> {
    use mg_geom::skeleton::{ArcGeometry, Sweep};
    let eval = |e: &ast::Expr| {
        mg_eval::eval_subexpr(&ctx.model.hir, ctx.instance, Some(glyph), ctx.values, e)
    };
    let geometry = match &seg.center {
        Some(center) => ArcGeometry::Center(eval(center)?.as_pair()?),
        None => ArcGeometry::Radii {
            rx: eval(seg.rx.as_ref()?)?.as_num()?,
            ry: eval(seg.ry.as_ref()?)?.as_num()?,
            large: seg.large,
        },
    };
    let sweep = match seg.sweep? {
        mg_hir::model::Sweep::Ccw => Sweep::Ccw,
        mg_hir::model::Sweep::Cw => Sweep::Cw,
    };
    let em = ctx.model.hir.font.em.unwrap_or(1000) as f64;
    let tolerance = mg_geom::tolerance::Tolerances::for_em(em).arc;
    let (center, rx, ry) = mg_geom::skeleton::arc_ellipse(
        Point::new(from[0], from[1]),
        Point::new(to[0], to[1]),
        geometry,
        sweep,
        tolerance,
    )
    .ok()?;
    let radius_text = |e: &Option<ast::Expr>| {
        seg.center
            .is_none()
            .then(|| e.as_ref().map(|e| ctx.text(e.syntax())))
            .flatten()
    };
    Some(ArcInfo {
        center: pt(center),
        rx,
        ry,
        from,
        rx_expr: radius_text(&seg.rx),
        ry_expr: radius_text(&seg.ry),
    })
}

fn pt(p: Point) -> Pt {
    [p.x, p.y]
}

/// Each authored segment's end point and control points, `start` first.
/// An `arc` spans several cubic pieces; its drawn controls are omitted
/// (they are an approximation, not something the author placed).
fn authored_ends(skeleton: &mg_geom::skeleton::Skeleton) -> Vec<(Option<Pt>, Vec<Pt>)> {
    let els: Vec<PathEl> = skeleton
        .path
        .elements()
        .iter()
        .copied()
        .filter(|el| !matches!(el, PathEl::ClosePath))
        .collect();
    let mut out = Vec::new();
    let Some(PathEl::MoveTo(start)) = els.first() else {
        return out;
    };
    out.push((Some(pt(*start)), Vec::new()));
    let mut i = 1;
    for &count in &skeleton.piece_counts {
        let pieces = &els[i.min(els.len())..(i + count).min(els.len())];
        i += count;
        let Some(last) = pieces.last() else {
            out.push((None, Vec::new()));
            continue;
        };
        let end = last.end_point().map(pt);
        let controls = match (count, last) {
            (1, PathEl::QuadTo(c, _)) => vec![pt(*c)],
            (1, PathEl::CurveTo(c1, c2, _)) => vec![pt(*c1), pt(*c2)],
            _ => Vec::new(),
        };
        out.push((end, controls));
    }
    out
}

/// A value as the inspector prints it: numbers to one decimal place.
pub fn format_value(value: &Value) -> String {
    match value {
        Value::Num(n) => format_num(*n),
        Value::Pair(p) => format!("({}, {})", format_num(p.x), format_num(p.y)),
        Value::Zone(z) => format_num(z.y),
        Value::Line(_) => "line".to_string(),
        Value::Path(_) => "path".to_string(),
        other => other.to_string(),
    }
}

pub fn format_num(n: f64) -> String {
    let s = format!("{n:.1}");
    let s = s.strip_suffix(".0").unwrap_or(&s);
    if s == "-0" { "0".to_string() } else { s.to_string() }
}
