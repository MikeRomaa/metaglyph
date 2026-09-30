//! The HIR itself (spec §5.3–§5.8, §5.11): one typed, name-resolved record
//! per declaration, built by [`crate::lower`]. Every field that held an
//! expression in the CST keeps that expression as an [`ast::Expr`] handle
//! rather than a re-encoded tree — `mg-eval` (M3) walks these directly to
//! build its dependency graph, the same way [`ast`] itself costs nothing
//! beyond the CST (see `mg-syntax/src/ast.rs`).
//!
//! A field is `None` when it was omitted *or* when it was malformed enough
//! that lowering could not extract an expression; the accompanying
//! diagnostic (missing-required-field, or whatever parse/field error fired)
//! is the record of why. Downstream passes must treat `None` as "already
//! diagnosed," never re-report it.

use std::ops::Range;

use indexmap::IndexMap;
use mg_syntax::SyntaxNode;
use mg_syntax::ast;

/// A glyph's key in [`Hir::glyphs`]: `(name, glyphset)`, `None` for the
/// default set (spec plan M2: "glyphs (keyed by (name, glyphset))").
pub type GlyphKey = (String, Option<String>);

#[derive(Debug)]
pub struct Hir {
    pub font: FontDecl,
    pub params: IndexMap<String, ParamDecl>,
    pub metrics: IndexMap<String, MetricDecl>,
    pub lets: IndexMap<String, LetDecl>,
    pub glyphs: IndexMap<GlyphKey, GlyphDecl>,
    pub instances: IndexMap<String, InstanceDecl>,
    pub groups: IndexMap<String, GroupDecl>,
    pub kerns: Vec<KernDecl>,
}

#[derive(Debug)]
pub struct FontDecl {
    pub syntax: SyntaxNode,
    pub name: Option<String>,
    pub em: Option<i64>,
    /// Defaults to `"1.000"` (spec §5.6).
    pub version: String,
    pub designer: Option<String>,
    pub foundry: Option<String>,
    pub license: Option<String>,
}

#[derive(Debug)]
pub struct ParamDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    pub default: Option<ast::Expr>,
    /// The constant-evaluated value of `default`, when it is one (spec
    /// §5.6 requires it to be).
    pub default_value: Option<f64>,
    pub range: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Top,
    Bottom,
}

#[derive(Debug)]
pub struct MetricDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    pub y: Option<ast::Expr>,
    pub overshoot: Option<ast::Expr>,
    /// Defaults to `Top` (spec §5.6).
    pub align: Align,
}

#[derive(Debug)]
pub struct LetDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    pub value: Option<ast::Expr>,
    /// `None` until [`crate::type_check`] computes it; every let is visited
    /// during lowering, so this is always `Some` by the time lowering
    /// returns.
    pub ty: Option<crate::types::Type>,
}

#[derive(Debug)]
pub struct GlyphDecl {
    pub name: String,
    /// `None` for the default set (spec §5.6).
    pub glyphset: Option<String>,
    pub syntax: SyntaxNode,
    pub codepoint_expr: Option<ast::Expr>,
    pub codepoints: Vec<u32>,
    pub advance: Option<ast::Expr>,
    /// The left and right sidebearings (spec §12.1). With `advance`, a
    /// glyph declares one or two of the three.
    pub lsb: Option<ast::Expr>,
    pub rsb: Option<ast::Expr>,
    pub lets: IndexMap<String, LetDecl>,
    /// Declaration order (paths may be anonymous, so this cannot be an
    /// `IndexMap` keyed by name; see [`stable_id`](mg_syntax::stable_id)
    /// for how anonymous siblings are identified elsewhere).
    pub paths: Vec<PathDecl>,
    pub anchors: IndexMap<String, AnchorDecl>,
    pub components: Vec<ComponentDecl>,
}

impl GlyphDecl {
    pub fn path_named(&self, name: &str) -> Option<&PathDecl> {
        self.paths.iter().find(|p| p.name.as_deref() == Some(name))
    }
}

#[derive(Debug, Clone)]
pub struct CapsSpec {
    pub start: String,
    pub end: String,
}

#[derive(Debug)]
pub struct PathDecl {
    pub name: Option<String>,
    pub syntax: SyntaxNode,
    pub stroke: Option<ast::Expr>,
    pub fill: bool,
    pub caps: Option<CapsSpec>,
    /// Defaults to `"miter"` (spec §5.7).
    pub joins: String,
    pub join_at: IndexMap<String, String>,
    /// Segments in source order.
    pub segments: Vec<SegmentDecl>,
    pub closed: bool,
}

impl PathDecl {
    /// `stroke.is_some() || fill` (spec plan M2).
    pub fn renders(&self) -> bool {
        self.stroke.is_some() || self.fill
    }

    pub fn segment_named(&self, name: &str) -> Option<&SegmentDecl> {
        self.segments
            .iter()
            .find(|s| s.name.as_deref() == Some(name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Start,
    Line,
    Quad,
    Cube,
    Arc,
}

/// `arc`'s `sweep` field: the direction of travel from the current point
/// to `to` about `center` (spec §5.7, y-up). Resolved to this enum at HIR
/// time, the same way `joins`/`align` are — not deferred to evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep {
    Ccw,
    Cw,
}

impl Sweep {
    pub fn as_str(self) -> &'static str {
        match self {
            Sweep::Ccw => "ccw",
            Sweep::Cw => "cw",
        }
    }
}

#[derive(Debug)]
pub struct SegmentDecl {
    pub kind: SegmentKind,
    pub name: Option<String>,
    pub syntax: SyntaxNode,
    /// `start` only.
    pub at: Option<ast::Expr>,
    /// `line` / `quad` / `cube` / `arc`.
    pub to: Option<ast::Expr>,
    /// `quad` only; omitted only when the previous declaration is also a
    /// `quad` (spec §6.3 reflection).
    pub c: Option<ast::Expr>,
    /// `cube` only; omitted only when the previous declaration is also a
    /// `cube` (spec §6.3 reflection).
    pub c1: Option<ast::Expr>,
    /// `cube` only; required.
    pub c2: Option<ast::Expr>,
    /// `arc` only; centre mode — mutually exclusive with `rx`/`ry`.
    pub center: Option<ast::Expr>,
    /// `arc` only; radii mode — the horizontal radius, requires `ry`.
    pub rx: Option<ast::Expr>,
    /// `arc` only; radii mode — the vertical radius, requires `rx`.
    pub ry: Option<ast::Expr>,
    /// `arc` only; radii mode only. Resolved at HIR time like `sweep`;
    /// `false` when omitted or not in radii mode.
    pub large: bool,
    /// `arc` only; required.
    pub sweep: Option<Sweep>,
}

#[derive(Debug)]
pub struct AnchorDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    pub at: Option<ast::Expr>,
}

#[derive(Debug)]
pub struct ComponentDecl {
    pub syntax: SyntaxNode,
    pub glyph: Option<String>,
    pub offset: Option<ast::Expr>,
    pub transform: Option<ast::Expr>,
}

#[derive(Debug)]
pub struct InstanceDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    /// Param name -> override expression, for every declared param this
    /// instance overrides.
    pub overrides: IndexMap<String, ast::Expr>,
    pub slant: Option<ast::Expr>,
    pub glyphset: Option<String>,
    /// Defaults to the instance's own name (spec §5.6).
    pub style_name: String,
    /// Defaults to `400` (spec §5.6).
    pub weight_class: i64,
    /// Defaults to `5` (spec §5.6).
    pub width_class: i64,
}

#[derive(Debug)]
pub struct GroupDecl {
    pub name: String,
    pub syntax: SyntaxNode,
    pub glyphs: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum KernSide {
    Glyph(String),
    Group(String),
}

#[derive(Debug)]
pub struct KernDecl {
    pub syntax: SyntaxNode,
    /// The raw name written for `left`/`right`, kept alongside the
    /// resolved side so a pass-2 diagnostic can still point at it even
    /// when resolution fails.
    pub left_name: Option<String>,
    pub left_span: Option<Range<usize>>,
    pub right_name: Option<String>,
    pub right_span: Option<Range<usize>>,
    /// `None` until resolved against the glyph/group namespace in pass 2.
    pub left: Option<KernSide>,
    pub right: Option<KernSide>,
    pub by: Option<ast::Expr>,
}
