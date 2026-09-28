//! Every declaration in one file, and what each name in it refers to
//! (spec §5.11), for document symbols, go-to-definition, and references
//! (plan 4, L1).
//!
//! Built from the CST rather than the HIR. The HIR keeps expressions as
//! unresolved `ast::Expr` handles, so it records no use sites to navigate,
//! and it only exists for a file without syntax errors — while the CST
//! always exists, `ERROR` nodes and all, so the outline and navigation
//! keep working in a half-typed file. The resolution rules are the same
//! ones `mg-hir` applies:
//!
//! - a bare name in an expression: the enclosing glyph's scope (its
//!   `let`s, anchors, and path names), then the top level (params,
//!   metrics, `let`s)
//! - `glyphs.X`, and `glyphref`/`groupref` fields (`component (glyph:)`,
//!   `group (glyphs:)`, `kern (left:, right:)`): the glyph namespace,
//!   where a glyph name means its default-set declaration
//! - `glyphs.X.a`: glyph `X`'s anchor `a`
//! - `glyphset:`: the first glyph declaring that set
//! - `follows:`: a path in the same glyph
//! - `joinAt` keys: a segment of the enclosing path, or of the path it
//!   follows
//! - an `instance` override key: the param it overrides

use std::ops::Range;

use mg_syntax::ast::{self, AstNode};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

/// What a name refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Def {
    /// A param, metric, or top-level `let`.
    TopLevel(String),
    /// A glyph (its default-set declaration).
    Glyph(String),
    Group(String),
    /// A glyph set, declared by every glyph naming it.
    GlyphSet(String),
    /// A `let`, anchor, or path in glyph `glyph` (an index into
    /// [`Index::glyphs`]); the three share one namespace.
    GlyphLocal {
        glyph: usize,
        name: String,
    },
    /// A named segment of path `path` in glyph `glyph`.
    Segment {
        glyph: usize,
        path: usize,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    /// The whole declaration, trimmed of leading trivia.
    pub range: Range<usize>,
    /// The name token.
    pub name_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct PathEntry {
    pub decl: Option<Decl>,
    /// The whole path, for an anonymous one.
    pub range: Range<usize>,
    pub segments: Vec<Decl>,
    pub follows: Option<String>,
    /// Whether it declares its own segments, so `follows:` may name it.
    pub has_body: bool,
}

#[derive(Debug, Clone)]
pub struct GlyphEntry {
    pub decl: Decl,
    pub glyphset: Option<String>,
    pub lets: Vec<Decl>,
    pub anchors: Vec<Decl>,
    pub paths: Vec<PathEntry>,
}

#[derive(Debug, Clone)]
pub struct KernEntry {
    pub range: Range<usize>,
    pub left: Option<String>,
    pub right: Option<String>,
}

/// Every declaration in one file, in source order.
#[derive(Debug, Clone, Default)]
pub struct Index {
    pub font: Option<Decl>,
    /// `font.em`, when it is a plain number literal (spec §5.6 requires
    /// one), for converting `em`-suffixed literals.
    pub em: Option<f64>,
    pub params: Vec<Decl>,
    pub metrics: Vec<Decl>,
    pub lets: Vec<Decl>,
    pub instances: Vec<Decl>,
    pub groups: Vec<Decl>,
    pub glyphs: Vec<GlyphEntry>,
    pub kerns: Vec<KernEntry>,
}

fn range_of(token: &SyntaxToken) -> Range<usize> {
    token.text_range().into()
}

fn decl(node: &SyntaxNode, name: Option<SyntaxToken>) -> Option<Decl> {
    let name = name?;
    Some(Decl {
        name: name.text().to_string(),
        range: mg_syntax::trimmed_range(node),
        name_range: range_of(&name),
    })
}

/// The plain name a `name: value` field holds, when it is one.
fn ident_field(config: Option<ast::Config>, field: &str) -> Option<String> {
    config?
        .fields()
        .find(|f| f.name_token().is_some_and(|t| t.text() == field))
        .and_then(|f| f.value())
        .and_then(|value| match value {
            ast::Expr::Ident(ident) => ident.token().map(|t| t.text().to_string()),
            _ => None,
        })
}

impl Index {
    pub fn new(file: &ast::SourceFile) -> Self {
        let mut index = Index::default();
        for item in file.items() {
            match item.kind() {
                SyntaxKind::FONT => {
                    index.em = ast::Font::cast(item.clone())
                        .and_then(|font| font.config())
                        .and_then(|config| {
                            config
                                .fields()
                                .find(|f| f.name_token().is_some_and(|t| t.text() == "em"))
                        })
                        .and_then(|field| field.value())
                        .and_then(|value| match value {
                            ast::Expr::Literal(lit) => lit.token()?.text().parse().ok(),
                            _ => None,
                        });
                    index.font = Some(Decl {
                        name: "font".to_string(),
                        range: mg_syntax::trimmed_range(&item),
                        name_range: mg_syntax::trimmed_range(&item),
                    });
                }
                SyntaxKind::PARAM => index.params.extend(
                    ast::Param::cast(item.clone()).and_then(|p| decl(&item, p.name_token())),
                ),
                SyntaxKind::METRIC => index.metrics.extend(
                    ast::Metric::cast(item.clone()).and_then(|m| decl(&item, m.name_token())),
                ),
                SyntaxKind::LET_STMT => index.lets.extend(
                    ast::LetStmt::cast(item.clone()).and_then(|l| decl(&item, l.name_token())),
                ),
                SyntaxKind::INSTANCE => index.instances.extend(
                    ast::Instance::cast(item.clone()).and_then(|i| decl(&item, i.name_token())),
                ),
                SyntaxKind::GROUP => index.groups.extend(
                    ast::Group::cast(item.clone()).and_then(|g| decl(&item, g.name_token())),
                ),
                SyntaxKind::GLYPH => {
                    if let Some(glyph) = ast::Glyph::cast(item.clone())
                        && let Some(entry) = glyph_entry(&glyph)
                    {
                        index.glyphs.push(entry);
                    }
                }
                SyntaxKind::KERN => {
                    let kern = ast::Kern::cast(item.clone()).expect("KERN casts");
                    index.kerns.push(KernEntry {
                        range: mg_syntax::trimmed_range(&item),
                        left: ident_field(kern.config(), "left"),
                        right: ident_field(kern.config(), "right"),
                    });
                }
                _ => {}
            }
        }
        index
    }

    /// The default-set declaration of glyph `name`.
    pub fn default_glyph(&self, name: &str) -> Option<usize> {
        self.glyphs
            .iter()
            .position(|g| g.decl.name == name && g.glyphset.is_none())
    }

    /// The glyph whose declaration spans `offset`.
    pub fn glyph_at(&self, offset: usize) -> Option<usize> {
        self.glyphs
            .iter()
            .position(|g| g.decl.range.contains(&offset))
    }

    /// Where `def` is declared: its name token, or the whole declaration
    /// when it has none.
    pub fn definition(&self, def: &Def) -> Option<Range<usize>> {
        let find = |decls: &[Decl], name: &str| {
            decls
                .iter()
                .find(|d| d.name == name)
                .map(|d| d.name_range.clone())
        };
        match def {
            Def::TopLevel(name) => find(&self.params, name)
                .or_else(|| find(&self.metrics, name))
                .or_else(|| find(&self.lets, name)),
            Def::Glyph(name) => self
                .default_glyph(name)
                .map(|g| self.glyphs[g].decl.name_range.clone()),
            Def::Group(name) => find(&self.groups, name),
            Def::GlyphSet(set) => self
                .glyphs
                .iter()
                .find(|g| g.glyphset.as_deref() == Some(set))
                .map(|g| g.decl.name_range.clone()),
            Def::GlyphLocal { glyph, name } => {
                let glyph = self.glyphs.get(*glyph)?;
                find(&glyph.lets, name)
                    .or_else(|| find(&glyph.anchors, name))
                    .or_else(|| {
                        glyph
                            .paths
                            .iter()
                            .filter_map(|p| p.decl.as_ref())
                            .find(|d| d.name == *name)
                            .map(|d| d.name_range.clone())
                    })
            }
            Def::Segment { glyph, path, name } => {
                let path = self.glyphs.get(*glyph)?.paths.get(*path)?;
                find(&path.segments, name)
            }
        }
    }

    /// A bare name used as a value inside the glyph at `glyph` (if any):
    /// glyph scope first, then the top level (spec §5.11).
    fn value(&self, glyph: Option<usize>, name: &str) -> Option<Def> {
        if let Some(g) = glyph {
            let local = Def::GlyphLocal {
                glyph: g,
                name: name.to_string(),
            };
            if self.definition(&local).is_some() {
                return Some(local);
            }
        }
        let top = Def::TopLevel(name.to_string());
        self.definition(&top).is_some().then_some(top)
    }

    /// A name in the glyph namespace: a glyph, else a group.
    fn glyph_or_group(&self, name: &str) -> Option<Def> {
        if self.default_glyph(name).is_some() {
            Some(Def::Glyph(name.to_string()))
        } else if self.groups.iter().any(|g| g.name == name) {
            Some(Def::Group(name.to_string()))
        } else {
            None
        }
    }

    /// The path declared by the `PATH` node spanning `offset` in glyph
    /// `glyph`, as an index into its paths.
    pub fn path_at(&self, glyph: usize, offset: usize) -> Option<usize> {
        self.glyphs[glyph]
            .paths
            .iter()
            .position(|p| p.range.contains(&offset))
    }

    /// What the identifier token `token` refers to, if it names anything
    /// declared in this file. Built-in functions, constants, namespace
    /// roots, and fields of `font`/`glyph`/`instance`/`math` resolve to
    /// nothing.
    pub fn resolve(&self, token: &SyntaxToken) -> Option<Def> {
        if token.kind() != SyntaxKind::IDENT {
            return None;
        }
        let name = token.text();
        let offset = usize::from(token.text_range().start());
        let parent = token.parent()?;
        let glyph = self.glyph_at(offset);

        match parent.kind() {
            // A declaration's own name.
            SyntaxKind::PARAM | SyntaxKind::METRIC => Some(Def::TopLevel(name.to_string())),
            SyntaxKind::LET_STMT => Some(match glyph {
                Some(g) => Def::GlyphLocal {
                    glyph: g,
                    name: name.to_string(),
                },
                None => Def::TopLevel(name.to_string()),
            }),
            SyntaxKind::GLYPH => Some(Def::Glyph(name.to_string())),
            SyntaxKind::GROUP => Some(Def::Group(name.to_string())),
            SyntaxKind::PATH | SyntaxKind::ANCHOR => Some(Def::GlyphLocal {
                glyph: glyph?,
                name: name.to_string(),
            }),
            SyntaxKind::START
            | SyntaxKind::LINE
            | SyntaxKind::QUAD
            | SyntaxKind::CUBE
            | SyntaxKind::ARC
            | SyntaxKind::CLOSE => {
                let g = glyph?;
                Some(Def::Segment {
                    glyph: g,
                    path: self.path_at(g, offset)?,
                    name: name.to_string(),
                })
            }
            SyntaxKind::FIELD => self.resolve_field_name(&parent, name),
            SyntaxKind::MAP_ENTRY => self.resolve_join_at_key(&parent, glyph?, offset, name),
            SyntaxKind::MEMBER_EXPR => self.resolve_member(&ast::MemberExpr::cast(parent)?, name),
            SyntaxKind::IDENT_EXPR => self.resolve_ident_expr(&parent, glyph, name),
            _ => None,
        }
    }

    /// An `instance` override key names the param it overrides.
    fn resolve_field_name(&self, field: &SyntaxNode, name: &str) -> Option<Def> {
        let block = field.parent()?.parent()?;
        (block.kind() == SyntaxKind::INSTANCE && self.params.iter().any(|p| p.name == name))
            .then(|| Def::TopLevel(name.to_string()))
    }

    /// A `joinAt: { key: "…" }` key names a segment of its path, or of the
    /// path that one `follows`.
    fn resolve_join_at_key(
        &self,
        entry: &SyntaxNode,
        glyph: usize,
        offset: usize,
        name: &str,
    ) -> Option<Def> {
        let field = entry.parent()?.parent().and_then(ast::Field::cast)?;
        if field.name_token()?.text() != "joinAt" {
            return None;
        }
        let path = self.path_at(glyph, offset)?;
        let entry = &self.glyphs[glyph].paths[path];
        let owner = if entry.segments.iter().any(|s| s.name == name) {
            path
        } else {
            let target = entry.follows.as_deref()?;
            self.glyphs[glyph]
                .paths
                .iter()
                .position(|p| p.decl.as_ref().is_some_and(|d| d.name == target))?
        };
        Some(Def::Segment {
            glyph,
            path: owner,
            name: name.to_string(),
        })
    }

    /// `glyphs.X` names glyph `X`; `glyphs.X.a` names its anchor `a`.
    fn resolve_member(&self, member: &ast::MemberExpr, name: &str) -> Option<Def> {
        match member.receiver()? {
            ast::Expr::Ident(root) if root.token()?.text() == "glyphs" => self
                .glyph_or_group(name)
                .filter(|d| matches!(d, Def::Glyph(_))),
            ast::Expr::Member(inner) => {
                let ast::Expr::Ident(root) = inner.receiver()? else {
                    return None;
                };
                if root.token()?.text() != "glyphs" {
                    return None;
                }
                let glyph = self.default_glyph(inner.member_token()?.text())?;
                let local = Def::GlyphLocal {
                    glyph,
                    name: name.to_string(),
                };
                self.glyphs[glyph]
                    .anchors
                    .iter()
                    .any(|a| a.name == name)
                    .then_some(local)
            }
            _ => None,
        }
    }

    fn resolve_ident_expr(
        &self,
        ident: &SyntaxNode,
        glyph: Option<usize>,
        name: &str,
    ) -> Option<Def> {
        let parent = ident.parent()?;
        // A call's callee is a built-in function, never a declaration.
        if parent.kind() == SyntaxKind::CALL_EXPR {
            return None;
        }
        // A name that is a reference-typed field's value, directly or as
        // a `group (glyphs: [...])` element.
        let field = match parent.kind() {
            SyntaxKind::FIELD => ast::Field::cast(parent),
            SyntaxKind::LIST_EXPR => parent.parent().and_then(ast::Field::cast),
            _ => None,
        };
        if let Some(field) = field {
            let block = field.syntax().parent()?.parent()?.kind();
            match (block, field.name_token()?.text()) {
                (SyntaxKind::COMPONENT, "glyph") | (SyntaxKind::GROUP, "glyphs") => {
                    return self
                        .glyph_or_group(name)
                        .filter(|d| matches!(d, Def::Glyph(_)));
                }
                (SyntaxKind::KERN, "left" | "right") => return self.glyph_or_group(name),
                (SyntaxKind::GLYPH | SyntaxKind::INSTANCE, "glyphset") => {
                    return Some(Def::GlyphSet(name.to_string()))
                        .filter(|d| self.definition(d).is_some());
                }
                (SyntaxKind::PATH, "follows") => {
                    let local = Def::GlyphLocal {
                        glyph: glyph?,
                        name: name.to_string(),
                    };
                    return self.definition(&local).is_some().then_some(local);
                }
                _ => {}
            }
        }
        self.value(glyph, name)
    }

    /// Every identifier token in `root` that refers to `def`, in source
    /// order, including the declaration's own name token.
    pub fn references(&self, root: &SyntaxNode, def: &Def) -> Vec<Range<usize>> {
        root.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .filter(|t| self.resolve(t).as_ref() == Some(def))
            .map(|t| range_of(&t))
            .collect()
    }
}

fn glyph_entry(glyph: &ast::Glyph) -> Option<GlyphEntry> {
    let node = glyph.syntax();
    let glyph_decl = decl(node, glyph.name_token())?;
    let glyphset = ident_field(glyph.config(), "glyphset");
    let mut entry = GlyphEntry {
        decl: glyph_decl,
        glyphset,
        lets: Vec::new(),
        anchors: Vec::new(),
        paths: Vec::new(),
    };
    let Some(body) = glyph.body() else {
        return Some(entry);
    };
    for item in body.items() {
        match item.kind() {
            SyntaxKind::LET_STMT => entry
                .lets
                .extend(ast::LetStmt::cast(item.clone()).and_then(|l| decl(&item, l.name_token()))),
            SyntaxKind::ANCHOR => entry
                .anchors
                .extend(ast::Anchor::cast(item.clone()).and_then(|a| decl(&item, a.name_token()))),
            SyntaxKind::PATH => {
                let path = ast::Path::cast(item.clone()).expect("PATH casts");
                let segments = path
                    .body()
                    .map(|body| {
                        body.items()
                            .filter_map(|segment| {
                                let name = segment
                                    .children_with_tokens()
                                    .filter_map(|e| e.into_token())
                                    .find(|t| t.kind() == SyntaxKind::IDENT);
                                decl(&segment, name)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                entry.paths.push(PathEntry {
                    decl: decl(&item, path.name_token()),
                    range: mg_syntax::trimmed_range(&item),
                    segments,
                    follows: ident_field(path.config(), "follows"),
                    has_body: path.body().is_some(),
                });
            }
            _ => {}
        }
    }
    Some(entry)
}
