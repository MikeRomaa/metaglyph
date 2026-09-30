import { describe, expect, it } from "vitest";
import { coverageWindow, dragPick, type PickCell } from "./pick.ts";

// A B C D E, with B already in the font.
const cells: PickCell[] = [
    { cp: 0x41, inFont: false },
    { cp: 0x42, inFont: true },
    { cp: 0x43, inFont: false },
    { cp: 0x44, inFont: false },
    { cp: 0x45, inFont: false },
];

describe("coverageWindow", () => {
    it("shows everything that fits", () => {
        expect(coverageWindow(95, 200, 10)).toEqual([0, 95]);
    });

    it("centres a window on the focus, inside the set", () => {
        expect(coverageWindow(586, 100, 300)).toEqual([250, 350]);
        expect(coverageWindow(586, 100, 10)).toEqual([0, 100]);
        expect(coverageWindow(586, 100, 580)).toEqual([486, 586]);
    });
});

describe("dragPick", () => {
    it("picks the missing cells between anchor and pointer, either way", () => {
        expect(dragPick(cells, 0, 3, true, [])).toEqual([0x41, 0x43, 0x44]);
        // Picks stay in reading order whichever way the drag goes.
        expect(dragPick(cells, 3, 0, true, [])).toEqual([0x41, 0x43, 0x44]);
    });

    it("keeps the picks the drag started with", () => {
        expect(dragPick(cells, 2, 3, true, [0x45])).toEqual([0x45, 0x43, 0x44]);
        // Moving back shrinks the range to what the pointer covers now.
        expect(dragPick(cells, 2, 2, true, [0x45])).toEqual([0x45, 0x43]);
    });

    it("unpicks the range when the drag started on a picked cell", () => {
        expect(dragPick(cells, 2, 4, false, [0x41, 0x43, 0x44, 0x45])).toEqual([
            0x41,
        ]);
    });
});
