import type { PointerEvent as ReactPointerEvent } from "react";
import { useCallback, useState } from "react";
import styles from "./Resizer.module.css";

const REM = 16;

/** A panel width in rem, clamped to `[min, max]` and remembered per key. */
export function usePanelWidth(
    key: string,
    initial: number,
    min: number,
    max: number,
) {
    const storageKey = `metaglyph.panel.${key}`;
    const clamp = (w: number) => Math.min(max, Math.max(min, w));
    const [width, setWidth] = useState(() => {
        try {
            const stored = Number(localStorage.getItem(storageKey));
            if (stored > 0) return clamp(stored);
        } catch {
            // Storage blocked; use the default.
        }
        return initial;
    });
    const set = useCallback(
        (w: number) => {
            const next = Math.min(max, Math.max(min, w));
            setWidth(next);
            try {
                localStorage.setItem(storageKey, String(next));
            } catch {
                // Not remembered; harmless.
            }
        },
        [storageKey, min, max],
    );
    return [width, set, initial] as const;
}

/** A drag strip on a panel's inner edge. `edge` is the side it sits on:
 * "right" for a left-hand panel, "left" for a right-hand one. Double-click
 * restores the default width. */
export function Resizer({
    edge,
    width,
    onResize,
    initial,
}: {
    edge: "left" | "right";
    width: number;
    onResize: (rem: number) => void;
    initial: number;
}) {
    const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.preventDefault();
        const target = e.currentTarget;
        target.setPointerCapture(e.pointerId);
        const x0 = e.clientX;
        const sign = edge === "right" ? 1 : -1;
        const move = (ev: PointerEvent) =>
            onResize(width + (sign * (ev.clientX - x0)) / REM);
        const up = () => {
            target.removeEventListener("pointermove", move);
            target.removeEventListener("pointerup", up);
            target.removeEventListener("pointercancel", up);
            document.body.style.cursor = "";
        };
        target.addEventListener("pointermove", move);
        target.addEventListener("pointerup", up);
        target.addEventListener("pointercancel", up);
        document.body.style.cursor = "col-resize";
    };
    return (
        <div
            className={styles.handle}
            data-edge={edge}
            aria-hidden
            title="Drag to resize · double-click to reset"
            onPointerDown={onPointerDown}
            onDoubleClick={() => onResize(initial)}
        />
    );
}
