import { describe, expect, it } from "vitest";
import { fitView, viewBox } from "./viewport.ts";

const none = { top: 0, right: 0, bottom: 0, left: 0 };

describe("fitView", () => {
    it("centres and scales a box into the element", () => {
        const v = fitView([0, 0, 1000, 500], 200, 200, none);
        expect(v).toEqual({ cx: 500, cy: 250, scale: 0.2 });
        expect(viewBox(v, 200, 200)).toEqual([0, -750, 1000, 1000]);
    });

    it("centres the box in the area left free by insets", () => {
        const v = fitView([0, 0, 100, 100], 300, 100, { ...none, left: 200 });
        // 100 px free on the right: 1 px per unit, box centre 100 px right
        // of the element centre.
        expect(v.scale).toBe(1);
        expect(v.cx).toBe(-50);
        expect(v.cy).toBe(50);
    });
});
