import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseNames, search } from "./unicode.ts";

const names = parseNames(
    readFileSync(new URL("../data/unicode-names.txt", import.meta.url), "utf8"),
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
        expect(names.get(hits[0])?.length).toBeLessThanOrEqual(
            names.get(hits[hits.length - 1])?.length ?? 0,
        );
    });

    it("never offers surrogates or out-of-range codepoints", () => {
        expect(search("U+D800", names)).toEqual([]);
        expect(search("U+110000", names)).toEqual([]);
    });

    it("caps the results", () => {
        expect(search("letter", names, 50)).toHaveLength(50);
    });
});
