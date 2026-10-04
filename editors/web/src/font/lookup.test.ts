import { describe, expect, it } from "vitest";
import type { FontData, GlyphInfo, KernInfo } from "../engine/types.ts";
import {
    effectiveKern,
    fmt,
    glyphsForText,
    kernPairs,
    kernSample,
    spacing,
} from "./lookup.ts";

function glyph(
    name: string,
    cp: number,
    extra: Partial<GlyphInfo> = {},
): GlyphInfo {
    return {
        name,
        codepoints: [cp],
        variations: [],
        span: [0, 0],
        fields: {},
        outline: "",
        components: 0,
        errors: 0,
        ...extra,
    };
}

function kern(k: Partial<KernInfo>): KernInfo {
    return {
        leftGroup: false,
        rightGroup: false,
        expr: "",
        span: [0, 0],
        ...k,
    };
}

const font: FontData = {
    instance: "Regular",
    em: 1000,
    metrics: [],
    lets: [],
    glyphs: [glyph("A", 0x41), glyph("a", 0x61), glyph("c", 0x63)],
    groups: [{ name: "bowls", glyphs: ["a", "c"], span: [0, 0] }],
    kerns: [
        kern({ left: "A", right: "bowls", rightGroup: true, by: -20 }),
        kern({ left: "A", right: "c", by: -45 }),
    ],
};

describe("fmt", () => {
    it("prints at most one decimal with a real minus", () => {
        expect(fmt(833.3333)).toBe("833.3");
        expect(fmt(1000)).toBe("1000");
        expect(fmt(-266.66)).toBe("−266.7");
        expect(fmt(-0.01)).toBe("0");
        expect(fmt(undefined)).toBe("—");
    });
});

describe("effectiveKern", () => {
    it("prefers a glyph pair over a group pair", () => {
        expect(effectiveKern(font, "A", "c")).toEqual({
            value: -45,
            index: 1,
            level: "glyph",
        });
        expect(effectiveKern(font, "A", "a")?.value).toBe(-20);
        expect(effectiveKern(font, "a", "A")).toBeNull();
    });
});

describe("kernPairs", () => {
    it("lists every covered pair with the kern it gets", () => {
        const pairs = kernPairs(font, 0).map((p) => [
            p.left,
            p.right,
            p.effective?.index,
        ]);
        // A → c is the glyph pair's (index 1), not the group pair's.
        expect(pairs).toEqual([
            ["A", "a", 0],
            ["A", "c", 1],
        ]);
        expect(kernPairs(font, 2)).toEqual([]);
    });
});

describe("kernSample", () => {
    it("skips members a glyph pair overrides", () => {
        const reordered: FontData = {
            ...font,
            groups: [{ name: "bowls", glyphs: ["c", "a"], span: [0, 0] }],
        };
        // A → c belongs to the glyph pair; the group pair shows as A → a.
        expect(kernSample(reordered, 0)).toEqual(["A", "a"]);
        expect(kernSample(reordered, 1)).toEqual(["A", "c"]);
        expect(kernSample(reordered, 2)).toBeUndefined();
    });
});

describe("spacing", () => {
    it("derives placed bearings from ink, shift and advance", () => {
        const g = glyph("A", 0x41, {
            advance: 833,
            shift: 10,
            ink: [-25, 0, 525, 1000],
        });
        expect(spacing(g)).toEqual({ advance: 833, lsb: -15, rsb: 298 });
    });
});

describe("glyphsForText", () => {
    it("skips characters with no glyph", () => {
        expect(glyphsForText(font, "AxcA").map((g) => g.name)).toEqual([
            "A",
            "c",
            "A",
        ]);
    });
});
