//! `mg fmt`: a canonical pretty-printer over the CST.
//!
//! This regenerates layout from structure rather than preserving the
//! source's own spacing, so it does not have to leave a hand-aligned file
//! unchanged (plan M1) — only comments and "was there a blank line here"
//! survive from the original trivia. Because output depends only on the
//! parsed structure, formatting is idempotent by construction.
//!
//! Known gap: a comment nested inside a `( … )` config (between two
//! fields) is currently dropped rather than forcing that config onto
//! multiple lines. Not exercised by the conformance sample; worth fixing
//! before this handles arbitrary user source.

use crate::ast::{AstNode, Expr};
use crate::syntax_kind::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};

pub fn format(source: &str) -> String {
    let parsed = crate::parse(source);
    let root = parsed.syntax();
    let mut out = String::new();
    print_item_sequence(&root, 0, &mut out);
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Why [`format_checked`] declined to format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// The source has syntax errors; formatting around `ERROR` nodes is
    /// partial-text tolerance, which this formatter does not attempt.
    SyntaxErrors,
    /// The output would not keep every token and comment of the source —
    /// for instance a comment between two config fields, which
    /// [`format`] cannot yet place (see this module's known gap).
    WouldLoseText,
}

/// [`format`], for callers that apply the result unattended (an editor's
/// format-on-save): it declines rather than return output that drops or
/// changes anything but whitespace. Checked by reparsing the output and
/// comparing its significant tokens and its comments with the source's.
pub fn format_checked(source: &str) -> Result<String, FormatError> {
    let parsed = crate::parse(source);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == mg_diag::Severity::Error)
    {
        return Err(FormatError::SyntaxErrors);
    }
    let output = format(source);
    // Everything but whitespace and a trailing comma (one right before a
    // closing bracket), which the formatter normalizes away.
    let text_of = |root: &SyntaxNode| -> Vec<(SyntaxKind, String)> {
        let tokens: Vec<(SyntaxKind, String)> = root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() != WHITESPACE)
            .map(|t| (t.kind(), t.text().trim_end().to_string()))
            .collect();
        let significant_after = |i: usize| tokens[i + 1..].iter().find(|(k, _)| *k != COMMENT);
        tokens
            .iter()
            .enumerate()
            .filter(|&(i, (kind, _))| {
                *kind != COMMA
                    || !significant_after(i)
                        .is_some_and(|(next, _)| matches!(next, R_PAREN | R_BRACKET | R_BRACE))
            })
            .map(|(_, token)| token.clone())
            .collect()
    };
    if text_of(&parsed.syntax()) == text_of(&crate::parse(&output).syntax()) {
        Ok(output)
    } else {
        Err(FormatError::WouldLoseText)
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

/// The trivia tokens leading a node: its own children, up to the first
/// non-trivia element. The parser flushes pending trivia into whichever
/// node is about to open, so a declaration's leading comments and blank
/// lines are always its own first children, never the previous sibling's
/// trailing children.
fn leading_trivia(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .take_while(|e| matches!(e, rowan::NodeOrToken::Token(t) if t.kind().is_trivia()))
        .filter_map(|e| e.into_token())
        .collect()
}

/// The trivia tokens immediately before a container's own closing
/// delimiter (`}` or EOF) — its last child, which this excludes.
fn trivia_before_last_token(container: &SyntaxNode) -> Vec<SyntaxToken> {
    let elements: Vec<_> = container.children_with_tokens().collect();
    let mut trivia = Vec::new();
    for e in elements[..elements.len().saturating_sub(1)].iter().rev() {
        match e {
            rowan::NodeOrToken::Token(t) if t.kind().is_trivia() => trivia.push(t.clone()),
            _ => break,
        }
    }
    trivia.reverse();
    trivia
}

/// A leading-trivia sequence, broken into: a comment trailing the previous
/// line (one preceded by nothing but same-line whitespace, so it shared
/// the previous token's line); the remaining comments, each tagged with
/// whether a blank line precedes *it*; and whether a blank line precedes
/// whatever follows the trivia (the declaration, or the container's
/// closing delimiter). Tracking the blank line's position per gap, rather
/// than one flag for the whole sequence, matters when a comment block's
/// last blank line sits between the comments and the declaration.
struct TriviaBreakdown {
    trailing_for_previous: Option<String>,
    lines: Vec<(bool, String)>,
    blank_before_next: bool,
}

fn break_down_trivia(trivia: &[SyntaxToken], has_previous: bool) -> TriviaBreakdown {
    let mut idx = 0;
    let mut trailing_for_previous = None;
    if has_previous {
        let after_same_line_ws = if trivia
            .first()
            .is_some_and(|t| t.kind() == WHITESPACE && !t.text().contains('\n'))
        {
            1
        } else {
            0
        };
        if trivia
            .get(after_same_line_ws)
            .is_some_and(|t| t.kind() == COMMENT)
        {
            trailing_for_previous = Some(trivia[after_same_line_ws].text().to_string());
            idx = after_same_line_ws + 1;
        }
    }
    let mut lines = Vec::new();
    let mut pending_blank = false;
    for t in &trivia[idx..] {
        match t.kind() {
            WHITESPACE if t.text().matches('\n').count() >= 2 => pending_blank = true,
            COMMENT => {
                lines.push((pending_blank, t.text().to_string()));
                pending_blank = false;
            }
            _ => {}
        }
    }
    TriviaBreakdown {
        trailing_for_previous,
        lines,
        blank_before_next: pending_blank,
    }
}

/// Prints `container`'s direct child declarations (its own children, plus
/// whatever trivia trails the last one before its closing delimiter),
/// leaving the cursor at the end of the last line with no trailing
/// newline — the caller adds one only if something follows.
fn print_item_sequence(container: &SyntaxNode, indent: usize, out: &mut String) {
    let items: Vec<SyntaxNode> = container.children().collect();
    let mut at_start = true;
    for (i, item) in items.iter().enumerate() {
        let trivia = leading_trivia(item);
        let breakdown = break_down_trivia(&trivia, i > 0);
        if let Some(comment) = &breakdown.trailing_for_previous {
            out.push_str("  ");
            out.push_str(comment.trim_end());
        }
        for (blank_before, comment) in &breakdown.lines {
            if !at_start {
                out.push('\n');
                if *blank_before {
                    out.push('\n');
                }
            }
            push_indent(out, indent);
            out.push_str(comment.trim_end());
            at_start = false;
        }
        if !at_start {
            out.push('\n');
            if breakdown.blank_before_next {
                out.push('\n');
            }
        }
        push_indent(out, indent);
        print_declaration(item, indent, out);
        at_start = false;
    }

    let trailing = trivia_before_last_token(container);
    if trailing.is_empty() {
        return;
    }
    let breakdown = break_down_trivia(&trailing, !at_start);
    if let Some(comment) = &breakdown.trailing_for_previous {
        out.push_str("  ");
        out.push_str(comment.trim_end());
    }
    for (blank_before, comment) in &breakdown.lines {
        if !at_start {
            out.push('\n');
            if *blank_before {
                out.push('\n');
            }
        }
        push_indent(out, indent);
        out.push_str(comment.trim_end());
        at_start = false;
    }
    // Nothing follows the container's trailing trivia, so a final
    // `blank_before_next` (a blank line right before `}`/EOF) has nothing
    // left to separate and is dropped.
}

fn first_token(node: &SyntaxNode, pred: impl Fn(SyntaxKind) -> bool) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| pred(t.kind()))
}

fn print_declaration(node: &SyntaxNode, indent: usize, out: &mut String) {
    match node.kind() {
        LET_STMT => print_let_stmt(node, out),
        FONT | PARAM | METRIC | GLYPH | INSTANCE | GROUP | KERN | PATH | ANCHOR | COMPONENT
        | START | LINE | QUAD | CUBE | ARC | CLOSE => print_block(node, indent, out),
        // Malformed input (an `ERROR` node): best-effort, lossy fallback.
        _ => out.push_str(&node.text().to_string()),
    }
}

fn print_let_stmt(node: &SyntaxNode, out: &mut String) {
    out.push_str("let ");
    if let Some(name) = first_token(node, |k| k == IDENT) {
        out.push_str(name.text());
    }
    out.push_str(" = ");
    if let Some(value) = node.children().find_map(Expr::cast) {
        print_expr(&value, out);
    }
    out.push(';');
}

fn keyword_text(kind: SyntaxKind) -> &'static str {
    match kind {
        FONT => "font",
        PARAM => "param",
        METRIC => "metric",
        GLYPH => "glyph",
        INSTANCE => "instance",
        GROUP => "group",
        KERN => "kern",
        PATH => "path",
        ANCHOR => "anchor",
        COMPONENT => "component",
        START => "start",
        LINE => "line",
        QUAD => "quad",
        CUBE => "cube",
        ARC => "arc",
        CLOSE => "close",
        _ => unreachable!("print_block only called on declaration-keyword node kinds"),
    }
}

fn print_block(node: &SyntaxNode, indent: usize, out: &mut String) {
    out.push_str(keyword_text(node.kind()));
    if let Some(name) = first_token(node, |k| k == IDENT) {
        out.push(' ');
        out.push_str(name.text());
    }
    if let Some(config) = node.children().find(|n| n.kind() == CONFIG) {
        out.push(' ');
        print_config(&config, out);
    }
    if let Some(body) = node.children().find(|n| n.kind() == BODY) {
        out.push(' ');
        print_body(&body, indent, out);
    }
}

fn print_config(config: &SyntaxNode, out: &mut String) {
    out.push('(');
    for (i, field) in config.children().filter(|n| n.kind() == FIELD).enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        print_field(&field, out);
    }
    out.push(')');
}

fn print_field(field: &SyntaxNode, out: &mut String) {
    if let Some(name) = first_token(field, SyntaxKind::is_word) {
        out.push_str(name.text());
    }
    out.push_str(": ");
    if let Some(value) = field.children().find_map(Expr::cast) {
        print_expr(&value, out);
    }
}

fn print_body(body: &SyntaxNode, indent: usize, out: &mut String) {
    let has_items = body.children().next().is_some();
    let has_trailing_comment = trivia_before_last_token(body)
        .iter()
        .any(|t| t.kind() == COMMENT);
    if !has_items && !has_trailing_comment {
        out.push_str("{}");
        return;
    }
    out.push_str("{\n");
    print_item_sequence(body, indent + 1, out);
    out.push('\n');
    push_indent(out, indent);
    out.push('}');
}

/// A node's single significant token, skipping any trivia the parser
/// flushed into it before that token (`LITERAL` and `IDENT_EXPR` each wrap
/// exactly one).
fn only_token(node: &SyntaxNode) -> String {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map(|t| t.text().to_string())
        .unwrap_or_default()
}

fn print_expr(expr: &Expr, out: &mut String) {
    match expr {
        Expr::Literal(n) => out.push_str(&only_token(n.syntax())),
        Expr::Ident(n) => out.push_str(&only_token(n.syntax())),
        // Malformed input: lossy fallback, same as print_declaration's.
        Expr::Error(n) => out.push_str(&n.syntax().text().to_string()),
        Expr::Paren(n) => {
            out.push('(');
            if let Some(inner) = n.inner() {
                print_expr(&inner, out);
            }
            out.push(')');
        }
        Expr::Tuple(n) => {
            out.push('(');
            for (i, el) in n.elements().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(&el, out);
            }
            out.push(')');
        }
        Expr::List(n) => {
            out.push('[');
            for (i, el) in n.elements().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(&el, out);
            }
            out.push(']');
        }
        Expr::Map(n) => {
            out.push_str("{ ");
            for (i, entry) in n.entries().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if let Some(key) = entry.key_token() {
                    out.push_str(key.text());
                }
                out.push_str(": ");
                if let Some(value) = entry.value() {
                    print_expr(&value, out);
                }
            }
            out.push_str(" }");
        }
        Expr::Unary(n) => {
            if let Some(op) = n.op_token() {
                out.push_str(op.text());
            }
            if let Some(operand) = n.operand() {
                print_expr(&operand, out);
            }
        }
        Expr::Bin(n) => {
            if let Some(lhs) = n.lhs() {
                print_expr(&lhs, out);
            }
            out.push(' ');
            if let Some(op) = n.op_token() {
                out.push_str(op.text());
            }
            out.push(' ');
            if let Some(rhs) = n.rhs() {
                print_expr(&rhs, out);
            }
        }
        Expr::Call(n) => {
            if let Some(callee) = n.callee() {
                print_expr(&callee, out);
            }
            out.push('(');
            if let Some(args) = n.arg_list() {
                for (i, arg) in args.args().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    print_expr(&arg, out);
                }
            }
            out.push(')');
        }
        Expr::Member(n) => {
            if let Some(receiver) = n.receiver() {
                print_expr(&receiver, out);
            }
            out.push('.');
            if let Some(member) = n.member_token() {
                out.push_str(member.text());
            }
        }
        Expr::Range(n) => {
            if let Some(low) = n.low() {
                print_expr(&low, out);
            }
            out.push_str("..");
            if let Some(high) = n.high() {
                print_expr(&high, out);
            }
        }
    }
}
