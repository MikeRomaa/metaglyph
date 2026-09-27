//! The dependency graph (spec §4): one node per binding, built fresh for
//! each instance (spec plan M3: cross-glyph references resolve "per
//! instance glyph set," so which concrete glyph a `glyphs.<name>`
//! reference even means can differ between instances — building one
//! graph per instance sidesteps that instead of layering it on top of a
//! shared structure).
//!
//! Anonymous subexpressions are not nodes; a node's "defining
//! expression(s)" are whatever HIR fields it owns, walked by
//! [`collect_refs`] to find every other node it depends on.

use indexmap::{IndexMap, IndexSet};
use mg_hir::model::{ComponentDecl, GlyphDecl, Hir, InstanceDecl, SegmentDecl};
use mg_syntax::SyntaxNode;
use mg_syntax::ast;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeId {
    TopLevel(String),
    GlyphLocal(String, String),
    GlyphAdvance(String),
    GlyphBbox(String),
    PathRealized(String, usize),
    PathBbox(String, usize),
    Anchor(String, String),
    Kern(usize),
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeId::TopLevel(name) => write!(f, "{name}"),
            NodeId::GlyphLocal(glyph, name) => write!(f, "{glyph}.{name}"),
            NodeId::GlyphAdvance(glyph) => write!(f, "{glyph}.advance"),
            NodeId::GlyphBbox(glyph) => write!(f, "{glyph}.bbox"),
            NodeId::PathRealized(glyph, i) => write!(f, "{glyph}.path[{i}]"),
            NodeId::PathBbox(glyph, i) => write!(f, "{glyph}.path[{i}].bbox"),
            NodeId::Anchor(glyph, name) => write!(f, "{glyph}.anchor({name})"),
            NodeId::Kern(i) => write!(f, "kern[{i}]"),
        }
    }
}

pub struct Graph {
    /// Every node, in first-seen (declaration) order.
    pub nodes: IndexSet<NodeId>,
    /// `node`'s own dependencies — the edges Kahn's algorithm consumes.
    pub deps: IndexMap<NodeId, Vec<NodeId>>,
    /// Byte offset of the declaration each node comes from, for the
    /// tie-break Kahn's algorithm and the cycle report both need (spec
    /// §14: "file position in the input list, then position within the
    /// file"). A single-file build, so file position is constant; this is
    /// the "position within the file" half.
    pub decl_index: IndexMap<NodeId, usize>,
}

impl Graph {
    pub(crate) fn new() -> Self {
        Self {
            nodes: IndexSet::new(),
            deps: IndexMap::new(),
            decl_index: IndexMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, node: NodeId, decl_span_start: usize, deps: Vec<NodeId>) {
        self.decl_index
            .entry(node.clone())
            .or_insert(decl_span_start);
        self.deps.insert(node.clone(), deps);
        self.nodes.insert(node);
    }
}

/// The glyph this instance actually builds under `name` (spec §5.6,
/// §12.3): the alternate in the instance's glyph set when one exists,
/// otherwise the default-set glyph. Panics if neither exists, which
/// `mg-hir`'s "alternate without default" check (spec §13) already rules
/// out for any HIR with zero error diagnostics.
pub fn effective_glyph<'a>(
    hir: &'a Hir,
    instance: &InstanceDecl,
    name: &str,
) -> Option<&'a GlyphDecl> {
    if let Some(set) = &instance.glyphset
        && let Some(glyph) = hir.glyphs.get(&(name.to_string(), Some(set.clone())))
    {
        return Some(glyph);
    }
    hir.glyphs.get(&(name.to_string(), None))
}

fn span_start(node: &SyntaxNode) -> usize {
    let range: std::ops::Range<usize> = node.text_range().into();
    range.start
}

/// Builds the full dependency graph for one instance: every top-level
/// param/metric/let, every glyph's locals/advance/bbox/paths/anchors, and
/// every kern.
pub fn build(hir: &Hir, instance: &InstanceDecl) -> Graph {
    let mut graph = Graph::new();

    for name in hir.params.keys() {
        // A param's value is always a constant expression (spec §5.6),
        // so it never depends on anything else in the graph.
        graph.insert(
            NodeId::TopLevel(name.clone()),
            span_start(&hir.params[name].syntax),
            vec![],
        );
    }

    for (name, metric) in &hir.metrics {
        let mut deps = Vec::new();
        if let Some(y) = &metric.y {
            collect_refs(y, hir, instance, None, &mut deps);
        }
        if let Some(overshoot) = &metric.overshoot {
            collect_refs(overshoot, hir, instance, None, &mut deps);
        }
        graph.insert(
            NodeId::TopLevel(name.clone()),
            span_start(&metric.syntax),
            deps,
        );
    }

    for (name, let_decl) in &hir.lets {
        let mut deps = Vec::new();
        if let Some(value) = &let_decl.value {
            collect_refs(value, hir, instance, None, &mut deps);
        }
        graph.insert(
            NodeId::TopLevel(name.clone()),
            span_start(&let_decl.syntax),
            deps,
        );
    }

    let mut glyph_names: IndexSet<&str> = IndexSet::new();
    for (name, _) in hir.glyphs.keys() {
        glyph_names.insert(name);
    }
    for name in glyph_names {
        // Only the glyph this instance actually selects under `name`
        // becomes nodes; the other variant (default vs. alternate) never
        // does, in this instance's graph.
        if let Some(glyph) = effective_glyph(hir, instance, name) {
            build_glyph(&mut graph, hir, instance, name, glyph);
        }
    }

    for (i, kern) in hir.kerns.iter().enumerate() {
        let mut deps = Vec::new();
        if let Some(by) = &kern.by {
            collect_refs(by, hir, instance, None, &mut deps);
        }
        graph.insert(NodeId::Kern(i), span_start(&kern.syntax), deps);
    }

    graph
}

fn build_glyph(
    graph: &mut Graph,
    hir: &Hir,
    instance: &InstanceDecl,
    name: &str,
    glyph: &GlyphDecl,
) {
    for (let_name, let_decl) in &glyph.lets {
        let mut deps = Vec::new();
        if let Some(value) = &let_decl.value {
            collect_refs(value, hir, instance, Some(name), &mut deps);
        }
        graph.insert(
            NodeId::GlyphLocal(name.to_string(), let_name.clone()),
            span_start(&let_decl.syntax),
            deps,
        );
    }

    for (anchor_name, anchor) in &glyph.anchors {
        let mut deps = Vec::new();
        if let Some(at) = &anchor.at {
            collect_refs(at, hir, instance, Some(name), &mut deps);
        }
        graph.insert(
            NodeId::Anchor(name.to_string(), anchor_name.clone()),
            span_start(&anchor.syntax),
            deps,
        );
    }

    let mut advance_deps = Vec::new();
    if let Some(advance) = &glyph.advance {
        collect_refs(advance, hir, instance, Some(name), &mut advance_deps);
    }
    graph.insert(
        NodeId::GlyphAdvance(name.to_string()),
        span_start(&glyph.syntax),
        advance_deps,
    );

    let mut bbox_deps = Vec::new();
    for (i, path) in glyph.paths.iter().enumerate() {
        let mut deps = Vec::new();
        for seg in &path.segments {
            collect_segment_refs(seg, hir, instance, name, &mut deps);
        }
        if let Some(target_name) = &path.follows
            && let Some(target_index) = glyph
                .paths
                .iter()
                .position(|p| p.name.as_deref() == Some(target_name.as_str()))
        {
            deps.push(NodeId::PathRealized(name.to_string(), target_index));
        }
        graph.insert(
            NodeId::PathRealized(name.to_string(), i),
            span_start(&path.syntax),
            deps,
        );

        graph.insert(
            NodeId::PathBbox(name.to_string(), i),
            span_start(&path.syntax),
            vec![NodeId::PathRealized(name.to_string(), i)],
        );
        if path.renders() {
            bbox_deps.push(NodeId::PathBbox(name.to_string(), i));
        }
    }

    for component in &glyph.components {
        collect_component_bbox_deps(component, hir, instance, name, &mut bbox_deps);
    }
    graph.insert(
        NodeId::GlyphBbox(name.to_string()),
        span_start(&glyph.syntax),
        bbox_deps,
    );
}

fn collect_component_bbox_deps(
    component: &ComponentDecl,
    hir: &Hir,
    instance: &InstanceDecl,
    current_glyph: &str,
    deps: &mut Vec<NodeId>,
) {
    if let Some(target) = &component.glyph
        && effective_glyph(hir, instance, target).is_some()
    {
        deps.push(NodeId::GlyphBbox(target.clone()));
    }
    if let Some(offset) = &component.offset {
        collect_refs(offset, hir, instance, Some(current_glyph), deps);
    }
    if let Some(transform) = &component.transform {
        collect_refs(transform, hir, instance, Some(current_glyph), deps);
    }
}

fn collect_segment_refs(
    seg: &SegmentDecl,
    hir: &Hir,
    instance: &InstanceDecl,
    current_glyph: &str,
    deps: &mut Vec<NodeId>,
) {
    let current_glyph = Some(current_glyph);
    let fields = [
        &seg.at,
        &seg.to,
        &seg.dir,
        &seg.from_dir,
        &seg.tension,
        &seg.curl,
    ];
    for expr in fields.into_iter().flatten() {
        collect_refs(expr, hir, instance, current_glyph, deps);
    }
    if let Some((c0, c1)) = &seg.controls {
        collect_refs(c0, hir, instance, current_glyph, deps);
        collect_refs(c1, hir, instance, current_glyph, deps);
    }
}

/// Walks `expr` for every bare-identifier or `glyphs.<name>.*` reference
/// that names a graph node, pushing each as a dependency. Mirrors
/// `mg_hir::type_check`'s own resolution order (glyph scope, then
/// top-level, then builtins) — see that module for why a `let`'s
/// resolution can't be a simple lookup table; here it doesn't need to be,
/// since we only want *which* node it is, not its value.
fn collect_refs(
    expr: &ast::Expr,
    hir: &Hir,
    instance: &InstanceDecl,
    current_glyph: Option<&str>,
    deps: &mut Vec<NodeId>,
) {
    use ast::Expr;
    match expr {
        Expr::Literal(_) | Expr::Error(_) => {}
        Expr::Ident(ident) => {
            let Some(token) = ident.token() else { return };
            let name = token.text();
            if let Some(glyph_name) = current_glyph
                && let Some(glyph) = effective_glyph(hir, instance, glyph_name)
            {
                if glyph.anchors.contains_key(name) {
                    deps.push(NodeId::Anchor(glyph_name.to_string(), name.to_string()));
                    return;
                }
                if let Some(i) = glyph
                    .paths
                    .iter()
                    .position(|p| p.name.as_deref() == Some(name))
                {
                    deps.push(NodeId::PathRealized(glyph_name.to_string(), i));
                    return;
                }
                if glyph.lets.contains_key(name) {
                    deps.push(NodeId::GlyphLocal(glyph_name.to_string(), name.to_string()));
                    return;
                }
            }
            if hir.params.contains_key(name)
                || hir.metrics.contains_key(name)
                || hir.lets.contains_key(name)
            {
                deps.push(NodeId::TopLevel(name.to_string()));
            }
            // Otherwise a built-in constant (spec §5.10): no dependency.
        }
        Expr::Paren(paren) => {
            if let Some(inner) = paren.inner() {
                collect_refs(&inner, hir, instance, current_glyph, deps);
            }
        }
        Expr::Tuple(tuple) => {
            for e in tuple.elements() {
                collect_refs(&e, hir, instance, current_glyph, deps);
            }
        }
        Expr::List(list) => {
            for e in list.elements() {
                collect_refs(&e, hir, instance, current_glyph, deps);
            }
        }
        Expr::Map(map) => {
            for entry in map.entries() {
                if let Some(v) = entry.value() {
                    collect_refs(&v, hir, instance, current_glyph, deps);
                }
            }
        }
        Expr::Unary(unary) => {
            if let Some(operand) = unary.operand() {
                collect_refs(&operand, hir, instance, current_glyph, deps);
            }
        }
        Expr::Bin(bin) => {
            if let Some(lhs) = bin.lhs() {
                collect_refs(&lhs, hir, instance, current_glyph, deps);
            }
            if let Some(rhs) = bin.rhs() {
                collect_refs(&rhs, hir, instance, current_glyph, deps);
            }
        }
        Expr::Call(call) => {
            if let Some(list) = call.arg_list() {
                for arg in list.args() {
                    collect_refs(&arg, hir, instance, current_glyph, deps);
                }
            }
        }
        Expr::Range(range) => {
            if let Some(low) = range.low() {
                collect_refs(&low, hir, instance, current_glyph, deps);
            }
            if let Some(high) = range.high() {
                collect_refs(&high, hir, instance, current_glyph, deps);
            }
        }
        Expr::Member(member) => collect_member_refs(member, hir, instance, current_glyph, deps),
    }
}

fn collect_member_refs(
    member: &ast::MemberExpr,
    hir: &Hir,
    instance: &InstanceDecl,
    current_glyph: Option<&str>,
    deps: &mut Vec<NodeId>,
) {
    let Some(receiver) = member.receiver() else {
        return;
    };

    // `glyphs.<name>.<field>`: the field itself becomes the dependency,
    // not a dependency on `receiver` (which is the synthetic
    // `glyphs.<name>` sub-expression, naming no value of its own).
    if let ast::Expr::Member(inner) = &receiver
        && let Some(ast::Expr::Ident(root)) = inner.receiver()
        && root.token().as_ref().map(|t| t.text()) == Some("glyphs")
        && let Some(glyph_name_token) = inner.member_token()
        && let Some(field_token) = member.member_token()
    {
        let glyph_name = glyph_name_token.text();
        match field_token.text() {
            "advance" => deps.push(NodeId::GlyphAdvance(glyph_name.to_string())),
            "bbox" => deps.push(NodeId::GlyphBbox(glyph_name.to_string())),
            anchor => {
                if effective_glyph(hir, instance, glyph_name)
                    .is_some_and(|g| g.anchors.contains_key(anchor))
                {
                    deps.push(NodeId::Anchor(glyph_name.to_string(), anchor.to_string()));
                }
            }
        }
        return;
    }

    // `glyph.<field>` (current glyph): depends on this glyph's own
    // advance/bbox when the field needs one; `.name`/`.codepoints` are
    // static, no dependency.
    if let ast::Expr::Ident(root) = &receiver
        && root.token().as_ref().map(|t| t.text()) == Some("glyph")
        && let Some(field_token) = member.member_token()
        && let Some(glyph_name) = current_glyph
    {
        match field_token.text() {
            "advance" => deps.push(NodeId::GlyphAdvance(glyph_name.to_string())),
            "bbox" => deps.push(NodeId::GlyphBbox(glyph_name.to_string())),
            _ => {}
        }
        return;
    }

    // `font.*`, `instance.*`, `math.*` are all static per instance: no
    // graph dependency. Anything else is a member on a general value
    // (`.x`, `.bbox` on a path, `.y` on a metric, …); the dependency is
    // whatever the receiver itself resolves to.
    collect_refs(&receiver, hir, instance, current_glyph, deps);
}
