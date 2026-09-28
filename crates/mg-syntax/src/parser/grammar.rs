use super::{Parser, TokenSet};
use crate::syntax_kind::SyntaxKind;
use SyntaxKind::*;

pub(super) fn source_file(p: &mut Parser) {
    p.start_node(SOURCE_FILE);
    while !p.at(EOF) {
        // `recover_declaration` deliberately leaves a `}` unconsumed for
        // whichever `body()` call is waiting to close on it (see its own
        // doc comment) — but at the top level there is no such call, no
        // matching `{`, and nothing else will ever consume it. Without
        // this guard a stray top-level `}` (e.g. one orphaned by a
        // misspelled declaration keyword swallowing the real `{`) makes
        // `declaration` return having bumped nothing, and this loop spins
        // on the same token forever.
        if p.at(R_BRACE) {
            p.error_expected(TokenSet::new(&[LET_KW, GLYPH_KW, PATH_KW]));
            p.start_node(ERROR);
            p.bump();
            p.finish_node();
            continue;
        }
        declaration(p);
    }
    p.bump(); // EOF, flushing any trailing trivia first
    p.finish_node();
}

/// Parses one declaration: `let <name> = <expr>;`, or the generic block
/// `<kind> <name>? ( <config> )? { <body> }?`. Used uniformly at the top
/// level and inside any `{ }` body; the plan defers "which kinds may
/// appear where" to M2's field/shape validation.
fn declaration(p: &mut Parser) {
    match p.nth(0) {
        LET_KW => let_stmt(p),
        kind if kind.starts_declaration() => block(p, kind),
        _ => recover_declaration(p),
    }
}

fn let_stmt(p: &mut Parser) {
    p.start_node(LET_STMT);
    p.bump(); // let
    if p.at(IDENT) {
        p.bump();
    } else {
        p.error_expected(TokenSet::new(&[IDENT]));
    }
    if p.at(EQ) {
        p.bump();
    } else {
        p.error_expected(TokenSet::new(&[EQ]));
    }
    expr(p);
    if p.at(SEMICOLON) {
        p.bump();
    } else {
        p.error_expected(TokenSet::new(&[SEMICOLON]));
    }
    p.finish_node();
}

fn block_node_kind(keyword: SyntaxKind) -> SyntaxKind {
    match keyword {
        FONT_KW => FONT,
        PARAM_KW => PARAM,
        METRIC_KW => METRIC,
        GLYPH_KW => GLYPH,
        INSTANCE_KW => INSTANCE,
        GROUP_KW => GROUP,
        KERN_KW => KERN,
        PATH_KW => PATH,
        ANCHOR_KW => ANCHOR,
        COMPONENT_KW => COMPONENT,
        START_KW => START,
        LINE_KW => LINE,
        QUAD_KW => QUAD,
        CUBE_KW => CUBE,
        ARC_KW => ARC,
        CLOSE_KW => CLOSE,
        _ => unreachable!("block() called on a non-declaration keyword"),
    }
}

/// `<kind> <name>? ( <config> )? { <body> }?`. The name slot accepts only
/// a plain `IDENT`: a reserved word can never fill it, so `glyph font (…)`
/// deliberately does not consume `font` as a name (M2 reports the
/// resulting shape as its own error rather than this parser special-casing
/// every keyword as a possible identifier).
fn block(p: &mut Parser, keyword: SyntaxKind) {
    let node_kind = block_node_kind(keyword);
    p.start_node(node_kind);
    p.bump();
    if p.at(IDENT) {
        p.bump();
    }
    if p.at(L_PAREN) {
        config(p);
    }
    if p.at(L_BRACE) {
        body(p);
    }
    p.finish_node();
}

fn body(p: &mut Parser) {
    p.start_node(BODY);
    let open_span = p.current_span();
    p.bump(); // {
    while !p.at(R_BRACE) && !p.at(EOF) {
        declaration(p);
    }
    p.expect_closing(R_BRACE, open_span);
    p.finish_node();
}

const CONFIG_SYNC: TokenSet = TokenSet::new(&[COMMA, R_PAREN]);

fn config(p: &mut Parser) {
    p.start_node(CONFIG);
    let open_span = p.current_span();
    p.bump(); // (
    if !p.at(R_PAREN) {
        loop {
            field(p);
            match p.nth(0) {
                COMMA => {
                    p.bump();
                    if p.at(R_PAREN) {
                        break;
                    }
                }
                R_PAREN | EOF => break,
                R_BRACE => break, // unclosed `(`: let `expect(R_PAREN)` below report it
                kind if kind.starts_declaration() => break, // ditto, at a declaration boundary
                _ => {
                    // One bad token between fields: report and skip it, a
                    // single token at a time, so `(a: 1 deg, b: 2)` still
                    // recovers to parse `b`.
                    p.error_expected(CONFIG_SYNC);
                    p.start_node(ERROR);
                    if !p.at(EOF) {
                        p.bump();
                    }
                    p.finish_node();
                    if p.at(EOF) {
                        break;
                    }
                }
            }
        }
    }
    p.expect_closing(R_PAREN, open_span);
    p.finish_node();
}

fn field(p: &mut Parser) {
    p.start_node(FIELD);
    // A field name is a reserved word syntactically (`component`'s own
    // `glyph:` field, for one), so it lexes as a keyword; only the
    // declaration's own name slot is restricted to plain `IDENT`.
    if p.nth(0).is_word() {
        p.bump();
    } else {
        p.error_expected(TokenSet::new(&[IDENT]));
    }
    if p.at(COLON) {
        p.bump();
        field_value(p);
    } else {
        p.error_expected(TokenSet::new(&[COLON]));
    }
    p.finish_node();
}

/// A field's value is an expression, except `range` fields (spec §5.2:
/// `bound ".." bound`). Both bounds are parsed as ordinary expressions;
/// M2 enforces that each is a literal (suffixed) number.
fn field_value(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    expr(p);
    if p.at(DOTDOT) {
        p.bump();
        expr(p);
        p.start_node_at(checkpoint, RANGE_EXPR);
        p.finish_node();
    }
}

/// Recovers when the current token starts neither `let` nor a declaration
/// keyword: wraps an `ERROR` node and skips to the next `;` (consumed), or
/// to `}`/a declaration keyword/EOF (left for the caller), per the plan's
/// "recovery at `;`, `}`, and declaration-keyword boundaries."
fn recover_declaration(p: &mut Parser) {
    p.error_expected(TokenSet::new(&[LET_KW, GLYPH_KW, PATH_KW]));
    p.start_node(ERROR);
    if p.at(EOF) {
        p.finish_node();
        return;
    }
    loop {
        if p.at(SEMICOLON) {
            p.bump();
            break;
        }
        if p.at(R_BRACE) || p.at(EOF) || p.nth(0).starts_declaration() {
            break;
        }
        p.bump();
    }
    p.finish_node();
}

// ---------------------------------------------------------------------
// Expressions (spec §5.8), tightest first: postfix, `^`, unary, `*` `/`,
// `+` `-`, comparisons, `==` `!=`, `and`, `or`.

fn expr(p: &mut Parser) {
    parse_or(p);
}

macro_rules! left_assoc_level {
    ($name:ident, $next:ident, [$($op:ident),+]) => {
        fn $name(p: &mut Parser) {
            let checkpoint = p.checkpoint();
            $next(p);
            while matches!(p.nth(0), $($op)|+) {
                p.bump();
                $next(p);
                p.start_node_at(checkpoint, BIN_EXPR);
                p.finish_node();
            }
        }
    };
}

left_assoc_level!(parse_or, parse_and, [OR_KW]);
left_assoc_level!(parse_and, parse_equality, [AND_KW]);
left_assoc_level!(parse_equality, parse_comparison, [EQEQ, NEQ]);
left_assoc_level!(parse_comparison, parse_additive, [LT, LE, GT, GE]);
left_assoc_level!(parse_additive, parse_multiplicative, [PLUS, MINUS]);
left_assoc_level!(parse_multiplicative, parse_unary, [STAR, SLASH]);

/// Looser than `^` (level 2): `-2^2` parses as `-(2^2)`.
fn parse_unary(p: &mut Parser) {
    if matches!(p.nth(0), MINUS | NOT_KW) {
        p.start_node(UNARY_EXPR);
        p.bump();
        parse_unary(p);
        p.finish_node();
    } else {
        parse_power(p);
    }
}

/// Right-associative; its right operand recurses through `parse_unary` so
/// `2^-1` and `a^b^c` both parse per spec §5.8.
fn parse_power(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    parse_postfix(p);
    if p.at(CARET) {
        p.bump();
        parse_unary(p);
        p.start_node_at(checkpoint, BIN_EXPR);
        p.finish_node();
    }
}

fn parse_postfix(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    parse_primary(p);
    loop {
        if p.at(DOT) {
            p.bump();
            if p.at_ident_like() {
                p.bump();
            } else {
                p.error_expected(TokenSet::new(&[IDENT]));
            }
            p.start_node_at(checkpoint, MEMBER_EXPR);
            p.finish_node();
        } else if p.at(L_PAREN) {
            arg_list(p);
            p.start_node_at(checkpoint, CALL_EXPR);
            p.finish_node();
        } else {
            break;
        }
    }
}

fn arg_list(p: &mut Parser) {
    p.start_node(ARG_LIST);
    let open_span = p.current_span();
    p.bump(); // (
    if !p.at(R_PAREN) {
        loop {
            expr(p);
            if p.at(COMMA) {
                p.bump();
                if p.at(R_PAREN) {
                    break;
                }
            } else {
                break;
            }
        }
    }
    p.expect_closing(R_PAREN, open_span);
    p.finish_node();
}

const PRIMARY_START: TokenSet = TokenSet::new(&[
    NUMBER,
    NUMBER_HEX,
    NUMBER_CODEPOINT,
    NUMBER_CHAR,
    STRING,
    IDENT,
    L_PAREN,
    L_BRACKET,
    L_BRACE,
]);

fn parse_primary(p: &mut Parser) {
    match p.nth(0) {
        NUMBER | NUMBER_ANGLE | NUMBER_RATIO | NUMBER_HEX | NUMBER_CODEPOINT | NUMBER_CHAR
        | STRING | TRUE_KW | FALSE_KW => {
            p.start_node(LITERAL);
            p.bump();
            p.finish_node();
        }
        IDENT | GLYPH_KW | INSTANCE_KW | FONT_KW => {
            p.start_node(IDENT_EXPR);
            p.bump();
            p.finish_node();
        }
        L_PAREN => paren_or_tuple(p),
        L_BRACKET => list_literal(p),
        L_BRACE => map_literal(p),
        _ => {
            p.error_expected(PRIMARY_START);
            p.start_node(ERROR);
            if !p.at(EOF) {
                p.bump();
            }
            p.finish_node();
        }
    }
}

/// `( a )` is grouping; `( a, b, … )` is a `TUPLE_EXPR` (spec §5.2, §5.8).
/// The parser only tells them apart by the presence of a comma — typing
/// which kind of tuple it is (pair vs. transform sequence) is M2's job.
fn paren_or_tuple(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    let open_span = p.current_span();
    p.bump(); // (
    if p.at(R_PAREN) {
        p.bump();
        p.start_node_at(checkpoint, TUPLE_EXPR);
        p.finish_node();
        return;
    }
    expr(p);
    if p.at(COMMA) {
        while p.at(COMMA) {
            p.bump();
            if p.at(R_PAREN) {
                break;
            }
            expr(p);
        }
        p.expect_closing(R_PAREN, open_span);
        p.start_node_at(checkpoint, TUPLE_EXPR);
        p.finish_node();
    } else {
        p.expect_closing(R_PAREN, open_span);
        p.start_node_at(checkpoint, PAREN_EXPR);
        p.finish_node();
    }
}

fn list_literal(p: &mut Parser) {
    p.start_node(LIST_EXPR);
    let open_span = p.current_span();
    p.bump(); // [
    if !p.at(R_BRACKET) {
        loop {
            expr(p);
            if p.at(COMMA) {
                p.bump();
                if p.at(R_BRACKET) {
                    break;
                }
            } else {
                break;
            }
        }
    }
    p.expect_closing(R_BRACKET, open_span);
    p.finish_node();
}

/// `{ … }` in expression position is a map literal (spec §5.1: it is a
/// body only after a block header). Keys are plain identifiers, as used by
/// `caps` and `joinAt`.
fn map_literal(p: &mut Parser) {
    p.start_node(MAP_EXPR);
    let open_span = p.current_span();
    p.bump(); // {
    if !p.at(R_BRACE) {
        loop {
            map_entry(p);
            if p.at(COMMA) {
                p.bump();
                if p.at(R_BRACE) {
                    break;
                }
            } else {
                break;
            }
        }
    }
    p.expect_closing(R_BRACE, open_span);
    p.finish_node();
}

fn map_entry(p: &mut Parser) {
    p.start_node(MAP_ENTRY);
    if p.nth(0).is_word() {
        p.bump();
    } else {
        p.error_expected(TokenSet::new(&[IDENT]));
    }
    if p.at(COLON) {
        p.bump();
        expr(p);
    } else {
        p.error_expected(TokenSet::new(&[COLON]));
    }
    p.finish_node();
}
