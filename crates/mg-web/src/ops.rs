//! Edit ops (plan 5, §1.1): each reads the current text's CST and returns
//! [`TextEdit`]s for the editor to apply as one CodeMirror transaction.
//! The editor never builds `.mg` text itself.
//!
//! Ops address declarations by their source span (UTF-16, as the views
//! report it) in a given document version; a stale version or a text
//! with syntax errors is refused.

use mg_syntax::ast::{self, AstNode};
use mg_syntax::edit::{self, TextEdit};
use mg_syntax::index::{Def, Index};
use mg_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use serde::{Deserialize, Serialize};

use crate::offsets::Utf16Index;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    /// Rename the declaration at `span`, and every reference to it.
    Rename { span: [usize; 2], name: String },
    /// Delete the declaration at `span` (plan 5: dangling references
    /// become diagnostics, as intended).
    Delete { span: [usize; 2] },
}

/// One replacement, in UTF-16 offsets of the text the op ran against.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum EditResult {
    Ok { version: u32, changes: Vec<Change> },
    /// The text changed since `version`; ask again.
    Stale,
    /// The text has syntax errors; edits wait until it parses (plan 5,
    /// §1.1).
    ReadOnly,
    /// The op can't apply, with a message for the user.
    Invalid { message: String },
}

fn invalid(message: impl Into<String>) -> EditResult {
    EditResult::Invalid {
        message: message.into(),
    }
}

/// Runs `op` against `source` (document `version`).
pub fn run(source: &str, version: u32, op: &Op) -> EditResult {
    let parsed = mg_syntax::parse(source);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == mg_diag::Severity::Error)
    {
        return EditResult::ReadOnly;
    }
    let root = parsed.syntax();
    let offsets = Utf16Index::new(source);
    let span = |s: &[usize; 2]| offsets.to_byte(s[0])..offsets.to_byte(s[1]);

    let edits = match op {
        Op::Rename { span: s, name } => {
            let Some(node) = decl_at(&root, span(s)) else {
                return invalid("That declaration is no longer in the source.");
            };
            match rename(&root, &node, name) {
                Ok(edits) => edits,
                Err(message) => return invalid(message),
            }
        }
        Op::Delete { span: s } => {
            let Some(node) = decl_at(&root, span(s)) else {
                return invalid("That declaration is no longer in the source.");
            };
            vec![edit::remove_decl(source, &node)]
        }
    };

    EditResult::Ok {
        version,
        changes: edits
            .into_iter()
            .map(|e| Change {
                from: offsets.convert(e.range.start),
                to: offsets.convert(e.range.end),
                insert: e.text,
            })
            .collect(),
    }
}

fn is_decl(kind: SyntaxKind) -> bool {
    use SyntaxKind::*;
    matches!(
        kind,
        LET_STMT
            | PARAM
            | METRIC
            | GLYPH
            | INSTANCE
            | GROUP
            | KERN
            | PATH
            | ANCHOR
            | COMPONENT
            | START
            | LINE
            | QUAD
            | CUBE
            | ARC
            | CLOSE
    )
}

/// The declaration whose trivia-trimmed range is exactly `range`.
fn decl_at(root: &SyntaxNode, range: std::ops::Range<usize>) -> Option<SyntaxNode> {
    root.descendants()
        .filter(|n| is_decl(n.kind()))
        .find(|n| edit::node_range(n) == range)
}

fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::IDENT)
}

/// A rename's edits, or why it can't happen: the name must be valid and
/// not collide with a declaration it would duplicate or shadow (spec
/// §5.4, §5.11).
fn rename(root: &SyntaxNode, node: &SyntaxNode, name: &str) -> Result<Vec<TextEdit>, String> {
    let token = name_token(node).ok_or("This declaration has no name to change.")?;
    if token.text() == name {
        return Ok(Vec::new());
    }
    if let Some(why) = edit::invalid_name(name) {
        return Err(format!("`{name}` is {why}."));
    }
    let file = ast::SourceFile::cast(root.clone()).expect("the root is a SOURCE_FILE");
    let index = Index::new(&file);
    let def = index
        .resolve(&token)
        .ok_or("This declaration can't be renamed.")?;
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
                Some(format!("glyph {} already declares `{name}`.", glyph_name(*glyph)))
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
        Def::GlyphSet(_) => Some("Glyph sets can't be renamed here.".to_string()),
    };
    if let Some(message) = conflict {
        return Err(message);
    }
    Ok(edit::rename(root, &index, &def, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "let h = 1000;\nglyph A (advance: h) {\n    let stem0 = (0, 0);\n    let stem1 = (1, 1);\n    path stem (stroke: 50) {\n        start (at: stem0)\n        line  (to: stem1)\n    }\n}\n";

    fn span_of(text: &str) -> [usize; 2] {
        let start = SRC.find(text).unwrap();
        [start, start + text.len()]
    }

    fn applied(result: EditResult) -> String {
        let EditResult::Ok { changes, .. } = result else {
            panic!("{result:?}");
        };
        let edits: Vec<TextEdit> = changes
            .into_iter()
            .map(|c| TextEdit {
                range: c.from..c.to,
                text: c.insert,
            })
            .collect();
        edit::apply(SRC, &edits)
    }

    #[test]
    fn renames_a_local_and_its_uses() {
        let op = Op::Rename {
            span: span_of("let stem0 = (0, 0);"),
            name: "base".into(),
        };
        assert_eq!(
            applied(run(SRC, 3, &op)),
            SRC.replace("stem0", "base")
        );
    }

    #[test]
    fn refuses_bad_and_colliding_names() {
        let local = span_of("let stem0 = (0, 0);");
        for (name, message) in [
            ("glyph", "`glyph` is a reserved word."),
            ("2x", "`2x` is not an identifier."),
            ("stem1", "glyph A already declares `stem1`."),
            ("h", "`h` would shadow a top-level declaration."),
        ] {
            let op = Op::Rename {
                span: local,
                name: name.into(),
            };
            assert_eq!(run(SRC, 1, &op), invalid(message), "{name}");
        }
    }

    #[test]
    fn deletes_a_declaration() {
        let op = Op::Delete {
            span: span_of("let stem1 = (1, 1);"),
        };
        assert_eq!(
            applied(run(SRC, 1, &op)),
            SRC.replace("    let stem1 = (1, 1);\n", "")
        );
    }

    #[test]
    fn refuses_text_with_syntax_errors() {
        let broken = format!("{SRC}glyph (");
        let op = Op::Delete { span: [0, 13] };
        assert_eq!(run(&broken, 1, &op), EditResult::ReadOnly);
    }

    #[test]
    fn a_span_that_matches_nothing_is_invalid() {
        let op = Op::Delete { span: [1, 3] };
        assert!(matches!(run(SRC, 1, &op), EditResult::Invalid { .. }));
    }
}
