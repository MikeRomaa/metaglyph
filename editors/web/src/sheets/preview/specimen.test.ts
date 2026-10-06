import { describe, expect, it } from "vitest";
import type { FontData, GlyphInfo } from "../../engine/types.ts";
import {
    fitGrid,
    inked,
    planSheets,
    specimenSheets,
    tidy,
} from "./specimen.ts";

function glyph(name: string, cp: number, outline = "M0 0L100 0L100 700Z") {
    return {
        name,
        codepoints: [cp],
        variations: [],
        span: [0, 0],
        advance: 500,
        fields: {},
        outline,
        components: 0,
        errors: 0,
    } satisfies GlyphInfo;
}

function font(n: number): FontData {
    return {
        instance: "Regular",
        em: 1000,
        metrics: [],
        glyphs: [
            glyph("space", 0x20, ""),
            ...Array.from({ length: n }, (_, i) => glyph(`g${i}`, 0x41 + i)),
        ],
        groups: [],
        kerns: [],
        lets: [],
    };
}

describe("specimen", () => {
    it("leaves out glyphs with no ink", () => {
        expect(inked(font(3)).map((g) => g.name)).toEqual(["g0", "g1", "g2"]);
    });

    it("fits a few glyphs in large cells on one sheet", () => {
        const grid = fitGrid(10, 380, 180);
        expect(grid?.cell).toBeLessThanOrEqual(40);
        expect((grid?.cols ?? 0) * (grid?.rows ?? 0)).toBeGreaterThanOrEqual(
            10,
        );
        expect(planSheets(inked(font(10)))).toHaveLength(1);
    });

    it("spreads many glyphs over sheets, samples on the first only", () => {
        const plans = planSheets(inked(font(1000)));
        expect(plans.length).toBeGreaterThan(1);
        expect(plans.map((p) => p.samples)).toEqual(
            plans.map((_, i) => i === 0),
        );
        expect(plans.reduce((n, p) => n + p.glyphs.length, 0)).toBe(1000);
    });

    it("escapes font info and numbers the sheets", () => {
        const sheets = specimenSheets(font(1000), {
            info: { name: "A<B> & Co", version: "1.0" },
            samples: [{ text: "ABC" }],
            kern: false,
            date: "2026-10-05",
        });
        expect(sheets[0]).toContain("A&#60;B&#62; &#38; CO");
        expect(sheets[0]).not.toContain("A<B>");
        expect(sheets.at(-1)).toContain(`${sheets.length} of ${sheets.length}`);
    });
});

describe("tidy", () => {
    it("rounds coordinates and flushes tiny values to 0", () => {
        expect(
            tidy(
                "M216.66666666666669,0 L216.6,-0.000000000000006123 L1e-15,-.5 Z",
            ),
        ).toBe("M216.67,0 L216.6,0 L0,-0.5 Z");
    });
});
