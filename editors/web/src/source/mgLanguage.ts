import type { StringStream } from "@codemirror/language";
import {
    HighlightStyle,
    StreamLanguage,
    syntaxHighlighting,
} from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

// Keywords as crates/mg-syntax/src/syntax_kind.rs lexes them.
export const KEYWORDS = new Set([
    "true",
    "false",
    "and",
    "or",
    "not",
    "font",
    "param",
    "metric",
    "let",
    "glyph",
    "instance",
    "group",
    "kern",
    "path",
    "anchor",
    "component",
    "start",
    "line",
    "quad",
    "cube",
    "arc",
    "close",
]);

/** One token's style name, or null for whitespace. Exported for tests. */
export function token(stream: StringStream): string | null {
    if (stream.eatSpace()) return null;
    if (stream.match("//")) {
        stream.skipToEnd();
        return "comment";
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
    // Character literal: its value is a codepoint number (spec §5.1).
    if (stream.match(/^'(?:\\.|[^'\\])'?/u)) return "string";
    if (stream.match(/^U\+[0-9A-Fa-f]+/) || stream.match(/^0x[0-9A-Fa-f]+/)) {
        return "number";
    }
    if (stream.match(/^\d+(?:\.\d+)?(?:deg|em)?/)) return "number";
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_]*/)) {
        return KEYWORDS.has(stream.current()) ? "keyword" : "variableName";
    }
    stream.next();
    return "punctuation";
}

export const mgLanguage = StreamLanguage.define({
    name: "metaglyph",
    token,
    languageData: { commentTokens: { line: "//" } },
});

// Colours from the design's tokenizer.
export const mgHighlight = syntaxHighlighting(
    HighlightStyle.define([
        { tag: t.keyword, color: "var(--ink)", fontWeight: "600" },
        { tag: t.variableName, color: "var(--ink)" },
        { tag: t.number, color: "var(--acc)" },
        { tag: t.string, color: "var(--str)" },
        { tag: t.comment, color: "var(--faint)" },
        { tag: t.punctuation, color: "var(--faint)" },
    ]),
);
