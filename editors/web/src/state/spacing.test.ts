import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FontData, GlyphInfo } from "../engine/types.ts";
import { parseNumber, typedSpacingOp } from "./spacing.ts";
import { useStore } from "./store.ts";

// The engine client starts a Web Worker on import.
vi.mock("../engine/client.ts", () => ({}));

/** Ink 100–400, placed so lsb is 40 and rsb 60 on a 400 advance. */
function glyph(fields: GlyphInfo["fields"]): GlyphInfo {
    return {
        name: "n",
        codepoints: [0x6e],
        span: [10, 20],
        advance: 400,
        shift: -60,
        ink: [100, 0, 400, 500],
        fields,
        outline: "",
        components: 0,
        errors: 0,
    };
}

function withGlyph(g: GlyphInfo) {
    const font: FontData = {
        instance: "Regular",
        em: 1000,
        metrics: [],
        lets: [],
        glyphs: [g],
        groups: [],
        kerns: [],
    };
    useStore.setState({ font, notice: null });
}

describe("parseNumber", () => {
    it("reads plain numbers only", () => {
        expect(parseNumber("42")).toBe(42);
        expect(parseNumber(" -1.5 ")).toBe(-1.5);
        expect(parseNumber("−12")).toBe(-12);
        expect(parseNumber(".5")).toBe(0.5);
        expect(parseNumber("side")).toBeNull();
        expect(parseNumber("1 + 2")).toBeNull();
        expect(parseNumber("")).toBeNull();
    });
});

describe("typedSpacingOp", () => {
    beforeEach(() => withGlyph(glyph({ advance: "400", lsb: "side" })));

    it("add-constants a declared field by the typed difference", () => {
        expect(typedSpacingOp("n", "lsb", "55")).toEqual({
            op: "addConstant",
            span: [10, 20],
            name: "lsb",
            delta: 15,
            em: 1000,
        });
    });

    it("splices an expression into a declared field", () => {
        expect(typedSpacingOp("n", "advance", "cell")).toEqual({
            op: "setField",
            span: [10, 20],
            name: "advance",
            value: { type: "expr", value: "cell" },
        });
    });

    it("moves the guide a derived field measures to", () => {
        expect(typedSpacingOp("n", "rsb", "50")).toEqual({
            op: "spacing",
            glyph: "n",
            edge: "right",
            delta: -10,
            lsb: 40,
            rsb: 60,
            em: 1000,
        });
    });

    it("refuses an expression for a derived field", () => {
        expect(typedSpacingOp("n", "rsb", "side")).toBeNull();
        expect(useStore.getState().notice?.text).toMatch(/derived/);
    });

    it("removes a field only while another stays declared", () => {
        expect(typedSpacingOp("n", "lsb", "")).toEqual({
            op: "removeField",
            span: [10, 20],
            name: "lsb",
        });
        withGlyph(glyph({ advance: "400" }));
        expect(typedSpacingOp("n", "advance", "")).toBeNull();
        expect(useStore.getState().notice?.text).toMatch(/one or two/);
    });
});
