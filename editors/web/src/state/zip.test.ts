import { describe, expect, it } from "vitest";
import { crc32, zip } from "./zip.ts";

const bytes = (s: string) => new TextEncoder().encode(s);

describe("crc32", () => {
    it("matches the standard check value", () => {
        expect(crc32(bytes("123456789"))).toBe(0xcbf43926);
    });
});

describe("zip", () => {
    it("stores each entry and ends with a directory of them", () => {
        const out = zip([
            { name: "a.ttf", data: bytes("hello") },
            { name: "b.ttf", data: bytes("world!") },
        ]);
        const view = new DataView(out.buffer);
        expect(view.getUint32(0, true)).toBe(0x04034b50);
        // The stored data follows the 30-byte header and the name.
        expect(new TextDecoder().decode(out.slice(35, 40))).toBe("hello");
        const end = out.length - 22;
        expect(view.getUint32(end, true)).toBe(0x06054b50);
        expect(view.getUint16(end + 10, true)).toBe(2);
        const directory = view.getUint32(end + 16, true);
        expect(view.getUint32(directory, true)).toBe(0x02014b50);
        // Reproducible: no clock in it.
        expect(zip([{ name: "a.ttf", data: bytes("hello") }])).toEqual(
            zip([{ name: "a.ttf", data: bytes("hello") }]),
        );
    });
});
