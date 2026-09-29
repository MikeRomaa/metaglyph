import { StringStream } from "@codemirror/language";
import { describe, expect, it } from "vitest";
import { token } from "./mgLanguage.ts";

function tokens(line: string) {
    const stream = new StringStream(line, 4, 4);
    const out: [string, string][] = [];
    while (!stream.eol()) {
        stream.start = stream.pos;
        const style = token(stream);
        if (style) out.push([stream.current(), style]);
    }
    return out;
}

describe("mg highlighter", () => {
    it("splits a let with a unit literal", () => {
        expect(tokens("let a = polar(p, 40, 37deg);")).toEqual([
            ["let", "keyword"],
            ["a", "variableName"],
            ["=", "punctuation"],
            ["polar", "variableName"],
            ["(", "punctuation"],
            ["p", "variableName"],
            [",", "punctuation"],
            ["40", "number"],
            [",", "punctuation"],
            ["37deg", "number"],
            [")", "punctuation"],
            [";", "punctuation"],
        ]);
    });

    it("reads codepoint forms as numbers or char literals", () => {
        const named = tokens(
            "glyph A (codepoint: 'A', x: U+0041, y: 0x41)",
        ).filter(([, style]) => style !== "punctuation");
        expect(named).toEqual([
            ["glyph", "keyword"],
            ["A", "variableName"],
            ["codepoint", "variableName"],
            ["'A'", "string"],
            ["x", "variableName"],
            ["U+0041", "number"],
            ["y", "variableName"],
            ["0x41", "number"],
        ]);
    });

    it("reads strings and trailing comments", () => {
        expect(tokens('font (name: "A // B") // note')).toEqual([
            ["font", "keyword"],
            ["(", "punctuation"],
            ["name", "variableName"],
            [":", "punctuation"],
            ['"A // B"', "string"],
            [")", "punctuation"],
            ["// note", "comment"],
        ]);
    });
});
