import { describe, expect, it } from "vitest";
import { glyphMarks } from "./nav.ts";

describe("glyphMarks", () => {
    it("finds every glyph declaration at the start of a line", () => {
        const text =
            'font (name: "T")\nglyph A (advance: 1) {\n}\n\nglyph zero_vs1 (advance: 1) {}\n';
        expect(glyphMarks(text).map((m) => m.pos)).toEqual([
            text.indexOf("glyph A"),
            text.indexOf("glyph zero_vs1"),
        ]);
    });

    it("ignores glyph references and comments", () => {
        expect(
            glyphMarks(
                "    component (glyph: e)\n// glyph X\nlet g = glyphs.A.advance;\n",
            ),
        ).toEqual([]);
    });

    it("labels a glyph with its character, from any codepoint spelling", () => {
        const text = [
            "glyph A (codepoint: 'A', advance: 1) {}",
            "glyph eacute (advance: 1,\n      codepoint: U+00E9) {}",
            "glyph quotesingle (codepoint: '\\'', advance: 1) {}",
            "glyph space (codepoint: ' ', advance: 1) {}",
            "glyph arrow (codepoint: 0x2192, advance: 1) {}",
            "glyph zero_vs1 (variation: ('0', U+FE00), advance: 1) {}",
        ].join("\n");
        expect(glyphMarks(text).map((m) => m.short)).toEqual([
            "A",
            "é",
            "'",
            "␣",
            "→",
            "zer",
        ]);
        expect(glyphMarks(text)[1].label).toBe("glyph eacute · U+00E9");
    });
});
