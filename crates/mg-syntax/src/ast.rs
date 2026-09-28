//! Typed AST layer over the CST (spec §5). Each node is a thin, `Copy`-free
//! newtype over a [`SyntaxNode`]; casting is a kind check, so this layer
//! costs nothing beyond the CST itself and stays trivially in sync with it.

use crate::syntax_kind::SyntaxKind;
use crate::{SyntaxNode, SyntaxToken};

pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                Self::can_cast(node.kind()).then_some(Self(node))
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

/// Adds the `<kind> <name>? ( <config> )? { <body> }?` accessors shared by
/// every declaration kind (spec §5.2). The name slot is deliberately only
/// ever an `IDENT` token (see the parser's `block()`), so this is exact,
/// not a best-effort search.
macro_rules! ast_block_node {
    ($name:ident, $kind:ident) => {
        ast_node!($name, $kind);

        impl $name {
            pub fn name_token(&self) -> Option<SyntaxToken> {
                self.0
                    .children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == SyntaxKind::IDENT)
            }

            pub fn config(&self) -> Option<Config> {
                self.0.children().find_map(Config::cast)
            }

            pub fn body(&self) -> Option<Body> {
                self.0.children().find_map(Body::cast)
            }
        }
    };
}

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(LetStmt, LET_STMT);
ast_node!(Config, CONFIG);
ast_node!(Field, FIELD);
ast_node!(Body, BODY);

ast_block_node!(Font, FONT);
ast_block_node!(Param, PARAM);
ast_block_node!(Metric, METRIC);
ast_block_node!(Glyph, GLYPH);
ast_block_node!(Instance, INSTANCE);
ast_block_node!(Group, GROUP);
ast_block_node!(Kern, KERN);
ast_block_node!(Path, PATH);
ast_block_node!(Anchor, ANCHOR);
ast_block_node!(Component, COMPONENT);
ast_block_node!(Start, START);
ast_block_node!(Line, LINE);
ast_block_node!(Quad, QUAD);
ast_block_node!(Cube, CUBE);
ast_block_node!(Arc, ARC);
ast_block_node!(Close, CLOSE);

ast_node!(Literal, LITERAL);
ast_node!(IdentExpr, IDENT_EXPR);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(TupleExpr, TUPLE_EXPR);
ast_node!(ListExpr, LIST_EXPR);
ast_node!(MapExpr, MAP_EXPR);
ast_node!(MapEntry, MAP_ENTRY);
ast_node!(UnaryExpr, UNARY_EXPR);
ast_node!(BinExpr, BIN_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(ArgList, ARG_LIST);
ast_node!(MemberExpr, MEMBER_EXPR);
ast_node!(RangeExpr, RANGE_EXPR);
ast_node!(ErrorNode, ERROR);

impl Literal {
    /// The literal's own token (spec §5.1). `start_node` opens this node
    /// before `bump` flushes pending trivia, so any whitespace or comment
    /// immediately before the literal is nested inside it as a leading
    /// token — this skips that, the same way [`UnaryExpr::op_token`] does.
    pub fn token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
    }

    /// The unescaped value of a `"…"` literal (spec §5.1: `\"` `\\` `\n`
    /// `\t`), or `None` when this literal isn't a string at all.
    pub fn string_value(&self) -> Option<String> {
        let token = self.token()?;
        if token.kind() != SyntaxKind::STRING {
            return None;
        }
        let inner = token.text().strip_prefix('"')?.strip_suffix('"')?;
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    _ => {}
                }
            } else {
                out.push(c);
            }
        }
        Some(out)
    }
}

impl IdentExpr {
    /// The identifier's own token; see [`Literal::token`] for why this
    /// isn't simply the node's first token.
    pub fn token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
    }
}

impl SourceFile {
    /// The file's top-level declarations, in source order.
    pub fn items(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children()
    }
}

impl Body {
    /// The body's declarations, in source order (order is significant for
    /// path segments; spec §5.2).
    pub fn items(&self) -> impl Iterator<Item = SyntaxNode> {
        self.0.children()
    }
}

impl LetStmt {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl Config {
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        self.0.children().filter_map(Field::cast)
    }
}

impl Field {
    /// The field name token. A reserved word syntactically (`component`'s
    /// own `glyph:` field, for one), so this is the first word-shaped
    /// token, not specifically an `IDENT`.
    pub fn name_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind().is_word())
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl MapEntry {
    pub fn key_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind().is_word())
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl UnaryExpr {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
    }

    pub fn operand(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
    }

    pub fn rhs(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn arg_list(&self) -> Option<ArgList> {
        self.0.children().find_map(ArgList::cast)
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

impl MemberExpr {
    pub fn receiver(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn member_token(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind().is_word())
            .last()
    }
}

impl RangeExpr {
    pub fn low(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn high(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl TupleExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

impl ListExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

impl MapExpr {
    pub fn entries(&self) -> impl Iterator<Item = MapEntry> {
        self.0.children().filter_map(MapEntry::cast)
    }
}

/// Any expression node. M2 does the real typing (spec §5.5, §5.8); this
/// layer only distinguishes syntactic shapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    Literal(Literal),
    Ident(IdentExpr),
    Paren(ParenExpr),
    Tuple(TupleExpr),
    List(ListExpr),
    Map(MapExpr),
    Unary(UnaryExpr),
    Bin(BinExpr),
    Call(CallExpr),
    Member(MemberExpr),
    Range(RangeExpr),
    Error(ErrorNode),
}

impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        use SyntaxKind::*;
        matches!(
            kind,
            LITERAL
                | IDENT_EXPR
                | PAREN_EXPR
                | TUPLE_EXPR
                | LIST_EXPR
                | MAP_EXPR
                | UNARY_EXPR
                | BIN_EXPR
                | CALL_EXPR
                | MEMBER_EXPR
                | RANGE_EXPR
                | ERROR
        )
    }

    fn cast(node: SyntaxNode) -> Option<Self> {
        use SyntaxKind::*;
        Some(match node.kind() {
            LITERAL => Expr::Literal(Literal(node)),
            IDENT_EXPR => Expr::Ident(IdentExpr(node)),
            PAREN_EXPR => Expr::Paren(ParenExpr(node)),
            TUPLE_EXPR => Expr::Tuple(TupleExpr(node)),
            LIST_EXPR => Expr::List(ListExpr(node)),
            MAP_EXPR => Expr::Map(MapExpr(node)),
            UNARY_EXPR => Expr::Unary(UnaryExpr(node)),
            BIN_EXPR => Expr::Bin(BinExpr(node)),
            CALL_EXPR => Expr::Call(CallExpr(node)),
            MEMBER_EXPR => Expr::Member(MemberExpr(node)),
            RANGE_EXPR => Expr::Range(RangeExpr(node)),
            ERROR => Expr::Error(ErrorNode(node)),
            _ => return None,
        })
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n) => n.syntax(),
            Expr::Ident(n) => n.syntax(),
            Expr::Paren(n) => n.syntax(),
            Expr::Tuple(n) => n.syntax(),
            Expr::List(n) => n.syntax(),
            Expr::Map(n) => n.syntax(),
            Expr::Unary(n) => n.syntax(),
            Expr::Bin(n) => n.syntax(),
            Expr::Call(n) => n.syntax(),
            Expr::Member(n) => n.syntax(),
            Expr::Range(n) => n.syntax(),
            Expr::Error(n) => n.syntax(),
        }
    }
}
