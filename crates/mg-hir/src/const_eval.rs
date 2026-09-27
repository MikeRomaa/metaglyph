//! Constant-expression evaluation (spec §5.2: "A constant expression
//! contains only literals, suffixed literals, arithmetic operators, and
//! `math.*`. It references no declaration."), used for `param default`,
//! `param range`, `instance` overrides, `glyph codepoint`, and the
//! integrality check on `int` fields once a value is known to be constant
//! (spec §5.3).
//!
//! This is deliberately narrower than full evaluation (M3): it folds only
//! the shapes the grammar allows in a constant expression, and returns
//! `None` for everything else, including a perfectly foldable expression
//! that references a `let` — such an expression is well-typed but not
//! *constant*, which is exactly the distinction spec §13's "non-constant
//! expression where one is required" diagnoses.

use mg_syntax::syntax_kind::SyntaxKind;
use mg_syntax::{SyntaxToken, ast};

/// Folds `expr` to a number if and only if it is a constant expression.
/// `font_em` is `font.em`'s own value, needed to fold an `em`-suffixed
/// literal; it is always available by the time any other constant is
/// folded, since `font` is lowered first.
pub fn eval_const(expr: &ast::Expr, font_em: Option<f64>) -> Option<f64> {
    use ast::Expr;
    match expr {
        Expr::Literal(lit) => literal_value(&lit.token()?, font_em),
        Expr::Paren(paren) => eval_const(&paren.inner()?, font_em),
        Expr::Unary(unary) => {
            let operand = eval_const(&unary.operand()?, font_em)?;
            match unary.op_token()?.kind() {
                SyntaxKind::MINUS => Some(-operand),
                _ => None,
            }
        }
        Expr::Bin(bin) => {
            let lhs = eval_const(&bin.lhs()?, font_em)?;
            let rhs = eval_const(&bin.rhs()?, font_em)?;
            match bin.op_token()?.kind() {
                SyntaxKind::PLUS => Some(lhs + rhs),
                SyntaxKind::MINUS => Some(lhs - rhs),
                SyntaxKind::STAR => Some(lhs * rhs),
                SyntaxKind::SLASH => Some(lhs / rhs),
                SyntaxKind::CARET => Some(if lhs == 0.0 && rhs == 0.0 {
                    1.0
                } else {
                    lhs.powf(rhs)
                }),
                _ => None,
            }
        }
        Expr::Member(member) => {
            let ast::Expr::Ident(receiver) = member.receiver()? else {
                return None;
            };
            if receiver.token()?.text() != "math" {
                return None;
            }
            crate::types::math_member(member.member_token()?.text())?;
            match member.member_token()?.text() {
                "pi" => Some(std::f64::consts::PI),
                "tau" => Some(std::f64::consts::TAU),
                "e" => Some(std::f64::consts::E),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The numeric value of a single literal token, applying its suffix
/// (spec §5.3: `deg` × π/180, `rad` × 1, `em` × `font.em`, `%` × 0.01).
/// Hex, codepoint, and character literals are exact integers, same value
/// regardless of spelling (spec §5.1).
fn literal_value(token: &SyntaxToken, font_em: Option<f64>) -> Option<f64> {
    let text = token.text();
    match token.kind() {
        SyntaxKind::NUMBER => text.parse().ok(),
        SyntaxKind::NUMBER_ANGLE => {
            if let Some(digits) = text.strip_suffix("deg") {
                digits
                    .parse::<f64>()
                    .ok()
                    .map(|v| v * std::f64::consts::PI / 180.0)
            } else {
                text.strip_suffix("rad").and_then(|d| d.parse().ok())
            }
        }
        SyntaxKind::NUMBER_RATIO => {
            if let Some(digits) = text.strip_suffix('%') {
                digits.parse::<f64>().ok().map(|v| v * 0.01)
            } else {
                let digits = text.strip_suffix("em")?;
                let value: f64 = digits.parse().ok()?;
                Some(value * font_em?)
            }
        }
        SyntaxKind::NUMBER_HEX => {
            let digits = text
                .strip_prefix("0x")
                .or_else(|| text.strip_prefix("0X"))?;
            u64::from_str_radix(digits, 16).ok().map(|v| v as f64)
        }
        SyntaxKind::NUMBER_CODEPOINT => {
            let digits = text
                .strip_prefix("U+")
                .or_else(|| text.strip_prefix("u+"))?;
            u32::from_str_radix(digits, 16).ok().map(|v| v as f64)
        }
        SyntaxKind::NUMBER_CHAR => char_literal_value(text).map(|c| c as u32 as f64),
        _ => None,
    }
}

/// The boolean analogue of [`eval_const`], for `fill`/`enabled`/`caps`
/// fields whose position-dependent structural checks (`fill` requiring a
/// closed path, foremost) need a concrete value before M3 exists to
/// evaluate one for real. Same narrowness: `None` for anything but a
/// literal, a negation, or `and`/`or` of two such values.
pub fn eval_const_bool(expr: &ast::Expr) -> Option<bool> {
    use ast::Expr;
    match expr {
        Expr::Literal(lit) => match lit.token()?.kind() {
            SyntaxKind::TRUE_KW => Some(true),
            SyntaxKind::FALSE_KW => Some(false),
            _ => None,
        },
        Expr::Paren(paren) => eval_const_bool(&paren.inner()?),
        Expr::Unary(unary) if unary.op_token()?.kind() == SyntaxKind::NOT_KW => {
            Some(!eval_const_bool(&unary.operand()?)?)
        }
        Expr::Bin(bin) => {
            let lhs = eval_const_bool(&bin.lhs()?)?;
            let rhs = eval_const_bool(&bin.rhs()?)?;
            match bin.op_token()?.kind() {
                SyntaxKind::AND_KW => Some(lhs && rhs),
                SyntaxKind::OR_KW => Some(lhs || rhs),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The single Unicode scalar value inside a `'…'` literal, honoring the
/// escapes the lexer already validated (spec §5.1: `\'` `\\` `\n` `\t`).
/// Only ever called on a well-formed literal (the lexer already reported
/// an unterminated, empty, or multi-scalar one), so any malformed input
/// here just yields `None` rather than a second diagnostic.
fn char_literal_value(text: &str) -> Option<char> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut chars = inner.chars();
    let value = match chars.next()? {
        '\\' => match chars.next()? {
            '\'' => '\'',
            '\\' => '\\',
            'n' => '\n',
            't' => '\t',
            _ => return None,
        },
        c => c,
    };
    chars.next().is_none().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_syntax::ast::AstNode;

    fn field_expr(src: &str) -> ast::Expr {
        let full = format!("param p (default: {src})");
        let parse = mg_syntax::parse(&full);
        parse
            .syntax()
            .descendants()
            .find_map(mg_syntax::ast::Field::cast)
            .and_then(|f| f.value())
            .expect("field value")
    }

    #[test]
    fn folds_arithmetic() {
        assert_eq!(eval_const(&field_expr("2 + 3 * 4"), None), Some(14.0));
    }

    #[test]
    fn folds_angle_suffix() {
        let value = eval_const(&field_expr("180deg"), None).unwrap();
        assert!((value - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn folds_em_suffix_using_font_em() {
        assert_eq!(eval_const(&field_expr("2em"), Some(1000.0)), Some(2000.0));
    }

    #[test]
    fn folds_hex_and_codepoint_and_char() {
        assert_eq!(eval_const(&field_expr("0x41"), None), Some(65.0));
        assert_eq!(eval_const(&field_expr("U+0041"), None), Some(65.0));
        assert_eq!(eval_const(&field_expr("'A'"), None), Some(65.0));
    }

    #[test]
    fn folds_math_constants() {
        let value = eval_const(&field_expr("math.pi"), None).unwrap();
        assert!((value - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn a_declaration_reference_is_not_constant() {
        assert_eq!(eval_const(&field_expr("stem"), None), None);
    }

    #[test]
    fn zero_to_the_zero_is_one() {
        assert_eq!(eval_const(&field_expr("0^0"), None), Some(1.0));
    }
}
