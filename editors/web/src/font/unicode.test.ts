import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { sequenceId } from "./chars.ts";
import { describe as describeChar, parseNames, search } from "./unicode.ts";

const data = (file: string) =>
    readFileSync(new URL(`../data/${file}`, import.meta.url), "utf8");
const names = parseNames(
    data("unicode-names.txt"),
    data("variation-sequences.txt"),
);

describe("search", () => {
    it("finds a codepoint in any notation", () => {
        for (const q of ["U+2192", "u+2192", "0x2192", "2192"]) {
            expect(search(q, names)[0]).toBe(0x2192);
        }
    });

    it("finds pasted characters, each once", () => {
        expect(search("→é→", names)).toEqual([0x2192, 0xe9]);
        expect(search("ß", names)[0]).toBe(0xdf);
    });

    it("finds characters outside the name table by codepoint or paste", () => {
        expect(search("U+4E00", names)).toEqual([0x4e00]);
        expect(search("一", names)).toEqual([0x4e00]);
    });

    it("matches names by word prefixes, in any order", () => {
        expect(search("arr right", names)).toContain(0x2192);
        expect(search("right arr", names)).toContain(0x2192);
        expect(search("dagger", names)).toContain(0x2020);
    });

    it("ranks an exact name first, then shorter names", () => {
        expect(search("rightwards arrow", names)[0]).toBe(0x2192);
        const hits = search("arrow", names);
        expect(names.chars.get(hits[0])?.length).toBeLessThanOrEqual(
            names.chars.get(hits[hits.length - 1])?.length ?? 0,
        );
    });

    it("never offers surrogates or out-of-range codepoints", () => {
        expect(search("U+D800", names)).toEqual([]);
        expect(search("U+110000", names)).toEqual([]);
    });

    it("caps the results", () => {
        expect(search("letter", names, 50)).toHaveLength(50);
    });

    it("finds a variation sequence by its codepoints", () => {
        const zeroVs1 = sequenceId(0x30, 0xfe00);
        for (const q of ["U+0030 U+FE00", "0030 FE00", "0x30 0xFE00"]) {
            expect(search(q, names)[0]).toBe(zeroVs1);
        }
        expect(search("U+0030 U+FE00", names)).toEqual([zeroVs1, 0x30]);
    });

    it("finds a pasted variation sequence", () => {
        expect(search("0︀", names)).toEqual([sequenceId(0x30, 0xfe00)]);
        expect(search("a0︀b", names)).toEqual([
            0x61,
            sequenceId(0x30, 0xfe00),
            0x62,
        ]);
    });

    it("finds a sequence by its base name and description", () => {
        expect(search("zero short diagonal", names)).toContain(
            sequenceId(0x30, 0xfe00),
        );
    });

    it("describes a sequence by base name and description", () => {
        expect(describeChar(sequenceId(0x30, 0xfe00), names)).toBe(
            "DIGIT ZERO · short diagonal stroke form",
        );
        expect(describeChar(sequenceId(0x30, 0xfe05), names)).toBe(
            "DIGIT ZERO · VS6 (U+FE05)",
        );
    });
});
