import { describe, expect, it } from "vitest";
import { whitespaceChanges } from "./whitespace.ts";

function apply(
    text: string,
    changes: { from: number; to: number; insert: string }[],
) {
    let out = text;
    for (const c of [...changes].reverse()) {
        out = out.slice(0, c.from) + c.insert + out.slice(c.to);
    }
    return out;
}

describe("whitespaceChanges", () => {
    it("changes only the whitespace runs that differ", () => {
        const before = "glyph A(advance:  1){\n  let x=1;\n}";
        const after = "glyph A (advance: 1) {\n    let x = 1;\n}";
        const changes = whitespaceChanges(before, after);
        expect(changes).not.toBeNull();
        expect(apply(before, changes ?? [])).toBe(after);
        // `glyph` and the newline before `}` are untouched.
        expect(changes?.every((c) => c.from > 5)).toBe(true);
    });

    it("handles leading and trailing whitespace", () => {
        const changes = whitespaceChanges("  let x = 1;", "let x = 1;\n");
        expect(apply("  let x = 1;", changes ?? [])).toBe("let x = 1;\n");
    });

    it("is null when anything but whitespace differs", () => {
        expect(whitespaceChanges("let x = 1;", "let y = 1;")).toBeNull();
        expect(whitespaceChanges("let x = 1;", "let x = 1;;")).toBeNull();
    });

    it("is empty for identical text", () => {
        expect(whitespaceChanges("a b", "a b")).toEqual([]);
    });
});
