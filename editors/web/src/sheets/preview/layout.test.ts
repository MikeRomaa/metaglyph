import { describe, expect, it } from "vitest";
import type { FontData, GlyphInfo, KernInfo } from "../../engine/types.ts";
import { caretX, indexAt, layout, lineOf, place, shape } from "./layout.ts";

function glyph(
    name: string,
    cp: number,
    advance: number,
    extra: Partial<GlyphInfo> = {},
): GlyphInfo {
    return {
        name,
        codepoints: [cp],
        variations: [],
        span: [0, 0],
        advance,
        fields: {},
        outline: "",
        components: 0,
        errors: 0,
        ...extra,
    };
}

const font = {
    instance: "Regular",
    em: 1000,
    metrics: [],
    glyphs: [
        glyph("A", 0x41, 600),
        glyph("V", 0x56, 600),
        glyph("space", 0x20, 250),
        glyph("zero", 0x30, 500),
        glyph("zero_vs1", 0, 500, {
            codepoints: [],
            variations: [[0x30, 0xfe00]],
        }),
    ],
    kerns: [
        {
            left: "A",
            right: "V",
            by: -80,
            leftGroup: false,
            rightGroup: false,
        } as KernInfo,
    ],
    groups: [],
    lets: [],
} as unknown as FontData;

const texts = (lines: ReturnType<typeof layout>) =>
    lines.map((l) => l.items.map((i) => i.text).join(""));

describe("preview layout", () => {
    it("maps characters to glyphs, and a missing one to a half-em box", () => {
        const shaped = shape(font, "AxV");
        expect(shaped.map((s) => s.glyph?.name ?? null)).toEqual([
            "A",
            null,
            "V",
        ]);
        expect(shaped[1].advance).toBe(500);
        expect(shaped.map((s) => s.index)).toEqual([0, 1, 2]);
    });

    it("joins a mapped variation sequence, and drops an unmapped selector", () => {
        const joined = shape(font, "0︀A");
        expect(joined.map((s) => s.glyph?.name)).toEqual(["zero_vs1", "A"]);
        expect(joined.map((s) => s.index)).toEqual([0, 2]);
        expect(shape(font, "0︁").map((s) => s.glyph?.name)).toEqual(["zero"]);
    });

    it("applies kerning only when asked", () => {
        const kerned = place(font, shape(font, "AV"), true);
        expect(kerned.items.map((i) => i.x)).toEqual([0, 520]);
        expect(kerned.width).toBe(1120);
        expect(place(font, shape(font, "AV"), false).width).toBe(1200);
    });

    it("wraps after spaces, which stay on their line, and keeps line breaks", () => {
        // "AV AV" is 1120 + 250 + 1120 = 2490 wide kerned.
        const lines = layout(font, "AV AV\nA", 2000, true);
        expect(texts(lines)).toEqual(["AV ", "AV", "A"]);
        expect(lines.map((l) => [l.start, l.end])).toEqual([
            [0, 3],
            [3, 5],
            [6, 7],
        ]);
        expect(layout(font, "AV AV", 3000, true)).toHaveLength(1);
    });

    it("gives a word wider than the line a line of its own", () => {
        expect(texts(layout(font, "AVAVAV A", 1000, true))).toEqual([
            "AVAVAV ",
            "A",
        ]);
    });

    it("keeps an empty line for an empty paragraph", () => {
        const lines = layout(font, "A\n\nV", 5000, true);
        expect(lines.map((l) => [l.start, l.end, l.items.length])).toEqual([
            [0, 1, 1],
            [2, 2, 0],
            [3, 4, 1],
        ]);
    });

    it("places the caret and finds the offset nearest a point", () => {
        const lines = layout(font, "AV AV", 2000, true);
        // At the wrap, the caret goes to the start of the next line.
        expect(lineOf(lines, 3)).toBe(1);
        expect(lineOf(lines, 2)).toBe(0);
        expect(caretX(lines[0], 1)).toBe(520);
        expect(caretX(lines[1], 5)).toBe(1120);
        expect(indexAt(lines[0], 500)).toBe(1);
        expect(indexAt(lines[1], 5000)).toBe(5);
    });

    it("draws a missing character with the font's notdef glyph", () => {
        const withNotdef = {
            ...font,
            glyphs: [
                ...font.glyphs,
                glyph("notdef", 0, 450, { codepoints: [] }),
            ],
        } as FontData;
        const [x, space] = shape(withNotdef, "x ");
        expect(x.glyph?.name).toBe("notdef");
        expect(x.missing).toBe(true);
        expect(x.advance).toBe(450);
        // An absent space stays blank.
        expect(space.glyph).toBeNull();
        expect(shape(withNotdef, "A")[0].missing).toBe(false);
    });
});
