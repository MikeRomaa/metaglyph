import { describe, expect, it } from "vitest";
import { charCode, charLabel, sequenceId, unpack, vsNumber } from "./chars.ts";

describe("char ids", () => {
    it("leave codepoints as they are", () => {
        expect(unpack(0x10ffff)).toEqual({ base: 0x10ffff });
        expect(charLabel(0x30)).toBe("0030");
    });

    it("round-trip every variation selector", () => {
        for (const selector of [0xfe00, 0xfe0f, 0xe0100, 0xe01ef]) {
            const id = sequenceId(0x10ffff, selector);
            expect(id).toBeGreaterThan(0x10ffff);
            expect(unpack(id)).toEqual({ base: 0x10ffff, selector });
        }
        expect(charCode(sequenceId(0x30, 0xfe00))).toBe("U+0030 U+FE00");
    });

    it("number the selectors VS1–VS256", () => {
        expect(vsNumber(0xfe00)).toBe(1);
        expect(vsNumber(0xfe0f)).toBe(16);
        expect(vsNumber(0xe0100)).toBe(17);
        expect(vsNumber(0xe01ef)).toBe(256);
        expect(vsNumber(0x30)).toBeNull();
    });
});
