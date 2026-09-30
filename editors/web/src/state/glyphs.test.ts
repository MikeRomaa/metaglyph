import { describe, expect, it, vi } from "vitest";
import type { FontData } from "../engine/types.ts";
import { defaultAdvance, glyphDecl, glyphName, nameErrors } from "./glyphs.ts";

// The engine client starts a Web Worker on import.
vi.mock("../engine/client.ts", () => ({}));

const font = {
    instance: "Regular",
    em: 1000,
    metrics: [],
    lets: [{ name: "h", expr: "1000", value: "1000", span: [0, 0] }],
    glyphs: [{ name: "A" }],
    groups: [],
    kerns: [],
} as unknown as FontData;

describe("glyphName", () => {
    it("uses the AGLFN, else uniXXXX", () => {
        expect(glyphName(0x41)).toBe("A");
        expect(glyphName(0xe9)).toBe("eacute");
        expect(glyphName(0x2c)).toBe("comma");
        expect(glyphName(0x0416)).toBe("uni0416");
        expect(glyphName(0x1f600)).toBe("u1F600");
    });
});

describe("nameErrors", () => {
    it("checks each name against the font and the list", () => {
        const rows = [
            { codepoint: 0x42, name: "B" },
            { codepoint: 0x41, name: "A" },
            { codepoint: 0x43, name: "line" },
            { codepoint: 0x44, name: "2x" },
            { codepoint: 0x45, name: "E" },
            { codepoint: 0x46, name: "E" },
            { codepoint: 0x47, name: "h" },
        ];
        expect(nameErrors(rows, font)).toEqual([
            null,
            "already declared",
            "a reserved word",
            "not an identifier",
            "listed twice",
            "listed twice",
            "already declared",
        ]);
    });
});

describe("defaultAdvance", () => {
    const withAdvances = (advances: (string | undefined)[], em = 1000) =>
        ({
            em,
            glyphs: advances.map((advance) => ({ fields: { advance } })),
        }) as unknown as FontData;

    it("takes the most common advance an empty glyph can use", () => {
        expect(
            defaultAdvance(
                withAdvances([
                    "600",
                    "w",
                    "w",
                    undefined,
                    "glyph.bbox.width",
                    "glyph.bbox.width",
                    "glyph.bbox.width",
                ]),
            ),
        ).toBe("w");
        expect(defaultAdvance(withAdvances(["a", "b"]))).toBe("a");
        expect(defaultAdvance(withAdvances([undefined], 1200))).toBe("600");
    });
});

describe("glyphDecl", () => {
    it("writes a char literal only for a visible, unescaped character", () => {
        expect(glyphDecl({ codepoint: 0x41, name: "A" }, "s")).toBe(
            "glyph A (codepoint: 'A', advance: s) {\n}",
        );
        expect(glyphDecl({ codepoint: 0x20, name: "space" }, "s")).toContain(
            "codepoint: U+0020",
        );
        expect(
            glyphDecl({ codepoint: 0x27, name: "quotesingle" }, "s"),
        ).toContain("codepoint: U+0027");
        expect(
            glyphDecl({ codepoint: 0x301, name: "acutecomb" }, "s"),
        ).toContain("codepoint: U+0301");
    });
});
