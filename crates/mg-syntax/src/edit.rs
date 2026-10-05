//! Structured edits as minimal text splices (plan 5, §1.2). Every
//! primitive reads the CST and returns [`TextEdit`]s against the text it
//! was parsed from; nothing here re-prints a node, so bytes outside the
//! edited ranges never change and the formatter never runs implicitly.
//!
//! Invariants the tests check for every primitive:
//! 1. bytes outside the edited ranges are unchanged;
//! 2. the result parses with no new syntax errors.

use std::ops::Range;

use crate::ast::{self, AstNode};
use crate::index::{Def, Index};
use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Replace `range` (bytes) with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub text: String,
}

impl TextEdit {
    fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    fn insert(at: usize, text: impl Into<String>) -> Self {
        Self::replace(at..at, text)
    }

    fn delete(range: Range<usize>) -> Self {
        Self::replace(range, "")
    }
}

/// Applies non-overlapping `edits` to `source`.
pub fn apply(source: &str, edits: &[TextEdit]) -> String {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse((e.range.start, e.range.end)));
    let mut out = source.to_string();
    for edit in sorted {
        out.replace_range(edit.range.clone(), &edit.text);
    }
    out
}

// ---------------------------------------------------------------------
// Ranges and lines

/// The range of `node` without leading or trailing trivia.
pub fn node_range(node: &SyntaxNode) -> Range<usize> {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia());
    let full: Range<usize> = node.text_range().into();
    let Some(first) = tokens.next() else {
        return full.start..full.start;
    };
    let last = tokens.last().unwrap_or_else(|| first.clone());
    let start: Range<usize> = first.text_range().into();
    let end: Range<usize> = last.text_range().into();
    start.start..end.end
}

fn token_range(token: &SyntaxToken) -> Range<usize> {
    token.text_range().into()
}

fn line_start(src: &str, pos: usize) -> usize {
    src[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// The index of the `\n` ending `pos`'s line, or the end of the text.
fn line_end(src: &str, pos: usize) -> usize {
    src[pos..].find('\n').map_or(src.len(), |i| pos + i)
}

/// The whitespace a line starts with.
fn indent_of(src: &str, pos: usize) -> &str {
    let start = line_start(src, pos);
    let line = &src[start..line_end(src, start)];
    &line[..line.len() - line.trim_start().len()]
}

fn is_blank(s: &str) -> bool {
    s.chars().all(|c| c == ' ' || c == '\t' || c == '\r')
}

/// The first and last non-trivia tokens directly inside `node`.
fn direct_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
}

/// The next non-trivia token after `range.end` in the file.
fn next_token(node: &SyntaxNode, at: usize) -> Option<SyntaxToken> {
    let root = node.ancestors().last()?;
    root.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia() && usize::from(t.text_range().start()) >= at)
}

/// The last non-trivia token ending at or before `at`.
fn prev_token(node: &SyntaxNode, at: usize) -> Option<SyntaxToken> {
    let root = node.ancestors().last()?;
    root.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia() && usize::from(t.text_range().end()) <= at)
        .last()
}

// ---------------------------------------------------------------------
// Expressions and literals

/// Replace an expression with `text`.
pub fn replace_expr(expr: &ast::Expr, text: &str) -> TextEdit {
    TextEdit::replace(node_range(expr.syntax()), text)
}

fn is_decimal_literal(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::NUMBER | SyntaxKind::NUMBER_ANGLE | SyntaxKind::NUMBER_RATIO
    )
}

/// A decimal literal split into its number and unit suffix: `37.5deg` →
/// `("37.5", "deg")`.
fn split_literal(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// A decimal literal's value in its own unit (`0.5em` → `0.5`).
pub fn literal_value(token: &SyntaxToken) -> Option<f64> {
    if !is_decimal_literal(token.kind()) {
        return None;
    }
    split_literal(token.text()).0.parse().ok()
}

fn decimals(number: &str) -> usize {
    number.split_once('.').map_or(0, |(_, frac)| frac.len())
}

fn format_decimals(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    // `-0.00` and friends: the sign is written separately.
    s.trim_start_matches('-').to_string()
}

/// The `-` of a unary minus applied directly to `token`'s literal.
fn negating_minus(token: &SyntaxToken) -> Option<SyntaxToken> {
    let literal = token.parent()?;
    let unary = ast::UnaryExpr::cast(literal.parent()?)?;
    let op = unary.op_token()?;
    (op.kind() == SyntaxKind::MINUS).then_some(op)
}

/// Rewrite one decimal literal to mean `value` (in the literal's own
/// unit), keeping its unit suffix and its decimal count. A leading unary
/// `-` stays in place; the sign flips by adding or removing it.
pub fn replace_literal(token: &SyntaxToken, value: f64) -> Vec<TextEdit> {
    let (number, suffix) = split_literal(token.text());
    let places = decimals(number);
    let magnitude = format_decimals(value.abs(), places);
    let negative = value < 0.0 && magnitude.chars().any(|c| matches!(c, '1'..='9'));
    let range = token_range(token);
    match negating_minus(token) {
        Some(minus) if !negative => vec![
            TextEdit::delete(usize::from(minus.text_range().start())..range.start),
            TextEdit::replace(range, format!("{magnitude}{suffix}")),
        ],
        Some(_) => vec![TextEdit::replace(range, format!("{magnitude}{suffix}"))],
        None if negative => vec![TextEdit::replace(range, format!("-{magnitude}{suffix}"))],
        None => vec![TextEdit::replace(range, format!("{magnitude}{suffix}"))],
    }
}

/// `delta` (raw units) in a literal's unit: `em` literals count ems, `%`
/// hundredths of an em.
fn in_unit(delta: f64, suffix: &str, em: f64) -> f64 {
    match suffix {
        "em" => delta / em,
        "%" => delta / em * 100.0,
        _ => delta,
    }
}

/// A number literal, optionally under a unary minus: its token and signed
/// value.
fn signed_literal(expr: &ast::Expr) -> Option<(SyntaxToken, f64)> {
    match expr {
        ast::Expr::Literal(lit) => {
            let token = lit.token()?;
            Some((token.clone(), literal_value(&token)?))
        }
        ast::Expr::Unary(unary) if unary.op_token()?.kind() == SyntaxKind::MINUS => {
            let ast::Expr::Literal(lit) = unary.operand()? else {
                return None;
            };
            let token = lit.token()?;
            Some((token.clone(), -literal_value(&token)?))
        }
        _ => None,
    }
}

/// Add `delta` (raw units) to an expression (plan 5, §1.2):
/// - a bare literal is rewritten;
/// - a trailing `+ <lit>` / `- <lit>` is updated, and dropped at zero;
/// - otherwise ` + <delta>` is appended, parenthesizing the original only
///   when its top operator binds looser than `+`.
///
/// Appended constants are whole units. `em` is the font's em, for
/// converting into an `em`- or `%`-suffixed literal.
pub fn add_constant(expr: &ast::Expr, delta: f64, em: f64) -> Vec<TextEdit> {
    if let Some((token, value)) = signed_literal(expr) {
        let (_, suffix) = split_literal(token.text());
        return replace_literal(&token, value + in_unit(delta, suffix, em));
    }

    if let ast::Expr::Bin(bin) = expr
        && let Some(op) = bin.op_token()
        && matches!(op.kind(), SyntaxKind::PLUS | SyntaxKind::MINUS)
        && let Some(ast::Expr::Literal(lit)) = bin.rhs()
        && let Some(token) = lit.token()
        && let Some(value) = literal_value(&token)
        && let Some(lhs) = bin.lhs()
    {
        let (number, suffix) = split_literal(token.text());
        let current = if op.kind() == SyntaxKind::PLUS {
            value
        } else {
            -value
        };
        let next = current + in_unit(delta, suffix, em);
        let magnitude = format_decimals(next.abs(), decimals(number));
        if !magnitude.chars().any(|c| matches!(c, '1'..='9')) {
            // Back to zero: drop ` + <lit>` entirely.
            return vec![TextEdit::delete(
                node_range(lhs.syntax()).end..token_range(&token).end,
            )];
        }
        let sign = if next < 0.0 { "-" } else { "+" };
        return vec![
            TextEdit::replace(token_range(&op), sign),
            TextEdit::replace(token_range(&token), format!("{magnitude}{suffix}")),
        ];
    }

    let whole = delta.round();
    if whole == 0.0 {
        return Vec::new();
    }
    let sign = if whole < 0.0 { "-" } else { "+" };
    let tail = format!(" {sign} {}", whole.abs());
    let range = node_range(expr.syntax());
    let loose = match expr {
        ast::Expr::Bin(bin) => bin.op_token().is_some_and(|op| {
            use SyntaxKind::*;
            matches!(
                op.kind(),
                LT | LE | GT | GE | EQEQ | NEQ | AND_KW | OR_KW | DOTDOT
            )
        }),
        ast::Expr::Range(_) => true,
        ast::Expr::Unary(unary) => unary
            .op_token()
            .is_some_and(|op| op.kind() == SyntaxKind::NOT_KW),
        _ => false,
    };
    if loose {
        vec![
            TextEdit::insert(range.start, "("),
            TextEdit::insert(range.end, format!("){tail}")),
        ]
    } else {
        vec![TextEdit::insert(range.end, tail)]
    }
}

/// The literal a unit toggle converts in `expr`: the expression itself
/// when it is a (signed) literal, else a trailing `+ <lit>` / `- <lit>`,
/// else a literal operand of a top-level `*` (under a leading `-`).
fn unit_literal(expr: &ast::Expr) -> Option<SyntaxToken> {
    if let Some((token, _)) = signed_literal(expr) {
        return Some(token);
    }
    // `-0.05em * k` parses as `-(0.05em * k)`.
    if let ast::Expr::Unary(unary) = expr
        && unary.op_token()?.kind() == SyntaxKind::MINUS
    {
        return unit_literal(&unary.operand()?);
    }
    let ast::Expr::Bin(bin) = expr else {
        return None;
    };
    let op = bin.op_token()?.kind();
    let literal = |e: Option<ast::Expr>| e.as_ref().and_then(signed_literal).map(|(t, _)| t);
    match op {
        SyntaxKind::PLUS | SyntaxKind::MINUS => literal(bin.rhs()),
        SyntaxKind::STAR => literal(bin.lhs()).or_else(|| literal(bin.rhs())),
        _ => None,
    }
}

/// Toggle the unit of `expr`'s literal (plan 5, §1.6 "Unit toggle"):
/// `-15` ↔ `-0.015em` for a 1000-unit em. The literal keeps its
/// precision: going to `em` adds the decimals `em` takes (3 for 1000),
/// coming back removes them. `None` when `expr` has no literal to convert
/// or it already has the unit.
pub fn convert_unit(expr: &ast::Expr, to_em: bool, em: f64) -> Option<Vec<TextEdit>> {
    let token = unit_literal(expr)?;
    let (number, suffix) = split_literal(token.text());
    let is_em = suffix == "em";
    if is_em == to_em || !(suffix.is_empty() || is_em) {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    let shift = em.log10().ceil().max(0.0) as usize;
    let (value, places, suffix) = if to_em {
        (value / em, decimals(number) + shift, "em")
    } else {
        (value * em, decimals(number).saturating_sub(shift), "")
    };
    let text = format_decimals(value, places);
    Some(vec![TextEdit::replace(
        token_range(&token),
        format!("{text}{suffix}"),
    )])
}

// ---------------------------------------------------------------------
// Lists

/// Append `text` to `list` (plan 5, §1.6 "Add group member"), comma-aware:
/// `, text` after the last element (` text` after a trailing comma), a new
/// aligned line on a multi-line list, `[text]` in an empty one (keeping
/// `[ ]`'s inner spaces).
pub fn list_insert(src: &str, list: &ast::ListExpr, text: &str) -> TextEdit {
    let elements: Vec<ast::Expr> = list.elements().collect();
    let tokens: Vec<SyntaxToken> = direct_tokens(list.syntax()).collect();
    let open = tokens.iter().find(|t| t.kind() == SyntaxKind::L_BRACKET);
    let close = tokens.iter().find(|t| t.kind() == SyntaxKind::R_BRACKET);
    let Some(last) = elements.last() else {
        let (Some(open), Some(close)) = (open, close) else {
            return TextEdit::insert(node_range(list.syntax()).end, text);
        };
        let inner = token_range(open).end..token_range(close).start;
        let padded = if src[inner.clone()].is_empty() {
            text.to_string()
        } else {
            format!(" {text} ")
        };
        return TextEdit::replace(inner, padded);
    };
    let last_range = node_range(last.syntax());
    let trailing_comma = tokens
        .iter()
        .find(|t| t.kind() == SyntaxKind::COMMA && token_range(t).start >= last_range.end);
    let previous_end = match elements.len() {
        1 => open.map_or(last_range.start, |t| token_range(t).end),
        n => node_range(elements[n - 2].syntax()).end,
    };
    if src[previous_end..last_range.start].contains('\n') {
        let pad = " ".repeat(last_range.start - line_start(src, last_range.start));
        return match trailing_comma {
            Some(comma) => TextEdit::insert(token_range(comma).end, format!("\n{pad}{text},")),
            None => TextEdit::insert(last_range.end, format!(",\n{pad}{text}")),
        };
    }
    match trailing_comma {
        Some(comma) => TextEdit::insert(token_range(comma).end, format!(" {text}")),
        None => TextEdit::insert(last_range.end, format!(", {text}")),
    }
}

/// Remove element `index` of `list`, one adjacent comma and the spaces
/// between them; an element alone on its line takes the line. `None`
/// when there is no such element.
pub fn list_remove(src: &str, list: &ast::ListExpr, index: usize) -> Option<Vec<TextEdit>> {
    let element = list.elements().nth(index)?;
    let range = node_range(element.syntax());
    let commas: Vec<SyntaxToken> = direct_tokens(list.syntax())
        .filter(|t| t.kind() == SyntaxKind::COMMA)
        .collect();
    let after = commas.iter().find(|c| token_range(c).start >= range.end);
    let before = commas
        .iter()
        .rev()
        .find(|c| token_range(c).end <= range.start);
    // Only a comma directly next to the element (nothing but trivia between).
    let after = after.filter(|c| src[range.end..token_range(c).start].trim().is_empty());
    let before = before.filter(|c| src[token_range(c).end..range.start].trim().is_empty());

    let end_with_comma = after.map_or(range.end, |c| token_range(c).end);
    let ls = line_start(src, range.start);
    let le = line_end(src, end_with_comma);
    if is_blank(&src[ls..range.start]) && is_blank(&src[end_with_comma..le]) {
        return Some(vec![TextEdit::delete(ls..(le + 1).min(src.len()))]);
    }
    if let Some(comma) = after {
        let mut end = token_range(comma).end;
        while src[end..].starts_with(' ') {
            end += 1;
        }
        return Some(vec![TextEdit::delete(range.start..end)]);
    }
    if let Some(comma) = before {
        return Some(vec![TextEdit::delete(token_range(comma).start..range.end)]);
    }
    Some(vec![TextEdit::delete(range)])
}

// ---------------------------------------------------------------------
// Config fields

fn config_of(decl: &SyntaxNode) -> Option<ast::Config> {
    decl.children().find_map(ast::Config::cast)
}

pub fn find_field(decl: &SyntaxNode, name: &str) -> Option<ast::Field> {
    config_of(decl)?
        .fields()
        .find(|f| f.name_token().is_some_and(|t| t.text() == name))
}

/// Set field `name` of declaration `decl` to `text` (plan 5, §1.2):
/// replace the value if the field exists; otherwise append it — `, name:
/// text` on a single-line config, a new line aligned to the previous
/// field on a multi-line one, `(name: text)` for an empty or missing
/// config.
pub fn set_field(src: &str, decl: &SyntaxNode, name: &str, text: &str) -> Vec<TextEdit> {
    if let Some(field) = find_field(decl, name) {
        return match field.value() {
            Some(value) => vec![replace_expr(&value, text)],
            None => {
                let end = node_range(field.syntax()).end;
                vec![TextEdit::insert(end, format!(" {text}"))]
            }
        };
    }
    let entry = format!("{name}: {text}");

    let Some(config) = config_of(decl) else {
        // No config at all: after the name, or after the keyword.
        let anchor = direct_tokens(decl)
            .take_while(|t| t.kind() != SyntaxKind::L_BRACE)
            .last();
        let at = anchor.map_or(node_range(decl).start, |t| token_range(&t).end);
        return vec![TextEdit::insert(at, format!(" ({entry})"))];
    };

    let fields: Vec<ast::Field> = config.fields().collect();
    let tokens: Vec<SyntaxToken> = direct_tokens(config.syntax()).collect();
    let open = tokens.iter().find(|t| t.kind() == SyntaxKind::L_PAREN);
    let Some(last) = fields.last() else {
        let at = open.map_or(node_range(config.syntax()).start, |t| token_range(t).end);
        return vec![TextEdit::insert(at, entry)];
    };

    let last_range = node_range(last.syntax());
    let trailing_comma = next_token(decl, last_range.end)
        .filter(|t| t.kind() == SyntaxKind::COMMA && t.parent().as_ref() == Some(config.syntax()));
    let previous_end = match fields.len() {
        1 => open.map_or(last_range.start, |t| token_range(t).end),
        n => node_range(fields[n - 2].syntax()).end,
    };
    let multi_line = src[previous_end..last_range.start].contains('\n');

    if multi_line {
        let column = last_range.start - line_start(src, last_range.start);
        let pad = " ".repeat(column);
        match trailing_comma {
            Some(comma) => vec![TextEdit::insert(
                token_range(&comma).end,
                format!("\n{pad}{entry},"),
            )],
            None => vec![TextEdit::insert(last_range.end, format!(",\n{pad}{entry}"))],
        }
    } else {
        match trailing_comma {
            Some(comma) => vec![TextEdit::insert(
                token_range(&comma).end,
                format!(" {entry}"),
            )],
            None => vec![TextEdit::insert(last_range.end, format!(", {entry}"))],
        }
    }
}

/// Remove field `name` from `decl`'s config (plan 5, §1.2): the field,
/// one adjacent comma and the whitespace between them; a field alone on
/// its line takes its line with it. `None` when there is no such field.
pub fn remove_field(src: &str, decl: &SyntaxNode, name: &str) -> Option<Vec<TextEdit>> {
    let field = find_field(decl, name)?;
    let range = node_range(field.syntax());
    let config = field.syntax().parent()?;
    let is_comma =
        |t: &SyntaxToken| t.kind() == SyntaxKind::COMMA && t.parent().as_ref() == Some(&config);
    let after = next_token(decl, range.end).filter(is_comma);
    let before = prev_token(decl, range.start).filter(is_comma);

    // Alone on its line (with its trailing comma): remove the line.
    let end_with_comma = after.as_ref().map_or(range.end, |c| token_range(c).end);
    let ls = line_start(src, range.start);
    let le = line_end(src, end_with_comma);
    if is_blank(&src[ls..range.start]) && is_blank(&src[end_with_comma..le]) {
        return Some(vec![TextEdit::delete(ls..(le + 1).min(src.len()))]);
    }

    if let Some(comma) = after {
        // `a: 1, b: 2` → `b: 2`: the field, its comma and the spaces after.
        let mut end = token_range(&comma).end;
        while src[end..].starts_with(' ') {
            end += 1;
        }
        return Some(vec![TextEdit::delete(range.start..end)]);
    }
    if let Some(comma) = before {
        return Some(vec![TextEdit::delete(token_range(&comma).start..range.end)]);
    }
    Some(vec![TextEdit::delete(range)])
}

// ---------------------------------------------------------------------
// Declarations

/// Insert `text` as a new declaration in `body` (plan 5, §1.2): on a new
/// line after `after`, copying its indentation; with no `after`, first
/// in the body.
pub fn insert_decl(
    src: &str,
    body: &ast::Body,
    after: Option<&SyntaxNode>,
    text: &str,
) -> TextEdit {
    if let Some(after) = after {
        let range = node_range(after);
        let indent = indent_of(src, range.start);
        return TextEdit::insert(range.end, format!("\n{indent}{text}"));
    }
    if let Some(first) = body.items().next() {
        let start = node_range(&first).start;
        let indent = indent_of(src, start).to_string();
        return TextEdit::insert(line_start(src, start), format!("{indent}{text}\n"));
    }
    insert_into_empty_body(src, body, text)
}

fn insert_into_empty_body(src: &str, body: &ast::Body, text: &str) -> TextEdit {
    let range = node_range(body.syntax());
    let outer = indent_of(src, range.start).to_string();
    let open_end = range.start + 1;
    let close = range.end - 1;
    let inner = format!("{outer}    ");
    if src[open_end..close].contains('\n') {
        TextEdit::insert(open_end, format!("\n{inner}{text}"))
    } else {
        // `{}` or `{ }`: open it up.
        TextEdit::replace(open_end..close, format!("\n{inner}{text}\n{outer}"))
    }
}

/// Insert a `let` into a glyph body (plan 5, §1.2): after the last
/// existing `let`; with none, first in the body followed by a blank line.
pub fn insert_let(src: &str, body: &ast::Body, text: &str) -> TextEdit {
    let last_let = body
        .items()
        .filter(|n| n.kind() == SyntaxKind::LET_STMT)
        .last();
    if let Some(last) = last_let {
        return insert_decl(src, body, Some(&last), text);
    }
    match body.items().next() {
        Some(first) => {
            let start = node_range(&first).start;
            let indent = indent_of(src, start).to_string();
            TextEdit::insert(line_start(src, start), format!("{indent}{text}\n\n"))
        }
        None => insert_into_empty_body(src, body, text),
    }
}

/// Insert a top-level declaration of `kind` (plan 5, §1.2): after the last
/// declaration of the same kind, or at the end of the file. Glyphs are
/// separated by one blank line.
pub fn insert_top_level(
    src: &str,
    file: &ast::SourceFile,
    kind: SyntaxKind,
    text: &str,
) -> TextEdit {
    let separator = if kind == SyntaxKind::GLYPH {
        "\n\n"
    } else {
        "\n"
    };
    if let Some(last) = file.items().filter(|n| n.kind() == kind).last() {
        let end = node_range(&last).end;
        return TextEdit::insert(end, format!("{separator}{text}"));
    }
    let content_end = src.trim_end().len();
    if content_end == 0 {
        return TextEdit::replace(0..src.len(), format!("{text}\n"));
    }
    TextEdit::replace(content_end..src.len(), format!("\n\n{text}\n"))
}

/// Remove a declaration (plan 5, §1.2): its line when it is alone on it,
/// collapsing a resulting double blank line (or a blank line left just
/// inside a `{`) to none.
pub fn remove_decl(src: &str, node: &SyntaxNode) -> TextEdit {
    let range = node_range(node);
    let ls = line_start(src, range.start);
    let le = line_end(src, range.end);
    if !(is_blank(&src[ls..range.start]) && is_blank(&src[range.end..le])) {
        let mut end = range.end;
        while src[end..].starts_with(' ') {
            end += 1;
        }
        return TextEdit::delete(range.start..end);
    }

    let mut end = (le + 1).min(src.len());
    let next_blank = end < src.len() && is_blank(&src[end..line_end(src, end)]);
    let prev_line = if ls == 0 {
        None
    } else {
        let start = line_start(src, ls - 1);
        Some(&src[start..ls - 1])
    };
    let prev_blank = prev_line.is_some_and(is_blank);
    let prev_opens = prev_line.is_some_and(|l| l.trim_end().ends_with('{'));
    if next_blank && (prev_blank || prev_opens) {
        end = (line_end(src, end) + 1).min(src.len());
    }
    TextEdit::delete(ls..end)
}

// ---------------------------------------------------------------------
// Names

/// Why a name can't be used, or `None` if it can: it must be an
/// identifier and not a reserved word (spec §5.4).
pub fn invalid_name(name: &str) -> Option<&'static str> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Some("not an identifier");
    }
    if SyntaxKind::keyword_from_text(name).is_some() {
        return Some("a reserved word");
    }
    None
}

/// The symbol `token` names — its declaration or any reference to it —
/// renamed to `name` with every reference, or why it can't be: the name
/// must be a valid identifier and must not duplicate or shadow another
/// declaration (spec §5.4, §5.11). Shared by the web editor's rename and
/// the language server's.
pub fn rename_symbol(
    root: &SyntaxNode,
    index: &Index,
    token: &SyntaxToken,
    name: &str,
) -> Result<Vec<TextEdit>, String> {
    let def = renameable(index, token)?;
    if token.text() == name {
        return Ok(Vec::new());
    }
    if let Some(why) = invalid_name(name) {
        return Err(format!("`{name}` is {why}."));
    }
    let taken = |d: Def| index.decl(&d).is_some();
    let glyph_name = |g: usize| index.glyphs[g].decl.name.clone();

    let conflict = match &def {
        Def::TopLevel(_) => {
            if taken(Def::TopLevel(name.to_string())) {
                Some(format!("`{name}` is already declared at the top level."))
            } else {
                (0..index.glyphs.len())
                    .find(|&g| {
                        taken(Def::GlyphLocal {
                            glyph: g,
                            name: name.to_string(),
                        })
                    })
                    .map(|g| format!("glyph {} already declares a local `{name}`.", glyph_name(g)))
            }
        }
        Def::GlyphLocal { glyph, .. } => {
            if taken(Def::GlyphLocal {
                glyph: *glyph,
                name: name.to_string(),
            }) {
                Some(format!(
                    "glyph {} already declares `{name}`.",
                    glyph_name(*glyph)
                ))
            } else if taken(Def::TopLevel(name.to_string())) {
                Some(format!("`{name}` would shadow a top-level declaration."))
            } else {
                None
            }
        }
        Def::Glyph(_) | Def::Group(_) => (taken(Def::Glyph(name.to_string()))
            || taken(Def::Group(name.to_string())))
        .then(|| format!("A glyph or group named `{name}` already exists.")),
        Def::Segment { glyph, path, .. } => taken(Def::Segment {
            glyph: *glyph,
            path: *path,
            name: name.to_string(),
        })
        .then(|| format!("This path already has a segment named `{name}`.")),
        Def::GlyphSet(_) => unreachable!("`renameable` refuses glyph sets"),
    };
    match conflict {
        Some(message) => Err(message),
        None => Ok(rename(root, index, &def, name)),
    }
}

/// What `token` names, if it can be renamed: any declaration but a glyph
/// set, which has no single declaration to rename.
pub fn renameable(index: &Index, token: &SyntaxToken) -> Result<Def, String> {
    match index.resolve(token) {
        None => Err("This isn't a name that can be renamed.".to_string()),
        Some(Def::GlyphSet(_)) => Err("Glyph sets can't be renamed.".to_string()),
        Some(def) => Ok(def),
    }
}

/// Rename `def` and every reference to it (plan 5, §1.2).
pub fn rename(root: &SyntaxNode, index: &Index, def: &Def, new_name: &str) -> Vec<TextEdit> {
    index
        .references(root, def)
        .into_iter()
        .map(|range| TextEdit::replace(range, new_name))
        .collect()
}
