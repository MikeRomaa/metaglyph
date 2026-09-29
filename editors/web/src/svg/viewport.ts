import { useEffect, useRef, useState } from "react";
import type { Pt } from "../engine/types.ts";

/** A box in font units (y-up): `[x0, y0, x1, y1]`. */
export type Box = [number, number, number, number];

/** Screen-space room to keep clear of overlays, in px. */
export interface Insets {
    top: number;
    right: number;
    bottom: number;
    left: number;
}

export interface Viewport {
    /** Font-unit point at the centre of the drawing. */
    cx: number;
    cy: number;
    /** Pixels per font unit. */
    scale: number;
}

export function fitView(
    box: Box,
    w: number,
    h: number,
    insets: Insets,
): Viewport {
    const bw = Math.max(box[2] - box[0], 1);
    const bh = Math.max(box[3] - box[1], 1);
    const aw = Math.max(w - insets.left - insets.right, 40);
    const ah = Math.max(h - insets.top - insets.bottom, 40);
    const scale = Math.min(aw / bw, ah / bh);
    // Centre the box in the free area, then express that as the point
    // under the element's centre.
    const cx = (box[0] + box[2]) / 2 - (insets.left - insets.right) / 2 / scale;
    const cy = (box[1] + box[3]) / 2 + (insets.top - insets.bottom) / 2 / scale;
    return { cx, cy, scale };
}

/** The SVG `viewBox` for a viewport; SVG y is font y negated. */
export function viewBox(v: Viewport, w: number, h: number): Box {
    const vw = w / v.scale;
    const vh = h / v.scale;
    return [v.cx - vw / 2, -v.cy - vh / 2, vw, vh];
}

/** Tracks an element's size and a zoomable, pannable viewport that fits
 * `box` until the user zooms or pans. */
export function useViewport(box: Box, insets: Insets) {
    const ref = useRef<SVGSVGElement>(null);
    const [size, setSize] = useState({ w: 0, h: 0 });
    const [manual, setManual] = useState<Viewport | null>(null);

    useEffect(() => {
        const el = ref.current;
        if (!el) return;
        const observer = new ResizeObserver(([entry]) => {
            const { width, height } = entry.contentRect;
            setSize({ w: width, h: height });
        });
        observer.observe(el);
        return () => observer.disconnect();
    }, []);

    const view = manual ?? fitView(box, size.w, size.h, insets);

    /** The font-unit point under a client position. */
    const toFont = (clientX: number, clientY: number): Pt => {
        const rect = ref.current?.getBoundingClientRect();
        if (!rect) return [0, 0];
        return [
            view.cx + (clientX - rect.left - rect.width / 2) / view.scale,
            view.cy - (clientY - rect.top - rect.height / 2) / view.scale,
        ];
    };

    const zoomAt = (clientX: number, clientY: number, factor: number) => {
        const [fx, fy] = toFont(clientX, clientY);
        const scale = Math.min(Math.max(view.scale * factor, 0.02), 50);
        // Keep the point under the pointer fixed.
        setManual({
            cx: fx - (fx - view.cx) * (view.scale / scale),
            cy: fy - (fy - view.cy) * (view.scale / scale),
            scale,
        });
    };

    const panBy = (dxPx: number, dyPx: number) =>
        setManual({
            cx: view.cx - dxPx / view.scale,
            cy: view.cy + dyPx / view.scale,
            scale: view.scale,
        });

    return {
        ref,
        size,
        view,
        toFont,
        zoomAt,
        panBy,
        reset: () => setManual(null),
    };
}
