import { useEffect } from "react";
import { redoEdit, undoEdit } from "../source/editor.ts";
import { deleteSelection, endPath } from "./actions.ts";
import { cycleDrag, cyclePreferred, dragging, endDrag } from "./drag.ts";
import { endNudges } from "./kerning.ts";
import { cancelRelate } from "./relate.ts";
import { useStore } from "./store.ts";

/** Whether a key event belongs to something the user is typing in, or
 * to an open modal: sheet shortcuts wait until it closes. */
export function isTyping(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    return !!target?.closest(
        'input, textarea, select, .cm-editor, [aria-modal="true"]',
    );
}

/** The axis Tab cycles: x, or y when x has no driver; Shift: y. */
function tabAxis(
    e: KeyboardEvent,
    axis: [number | null, number | null],
): 0 | 1 {
    if (e.shiftKey) return 1;
    return axis[0] === null ? 1 : 0;
}

/** Edit shortcuts outside the source pane (it has its own): undo and redo
 * act on the one text history; Delete removes the selection; Tab cycles a
 * point's driver (plan 5, §1.5); Escape cancels what is in progress. */
export function useEditShortcuts() {
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (isTyping(e)) return;
            const s = useStore.getState();
            const mod = e.ctrlKey || e.metaKey;
            const key = e.key.toLowerCase();

            if (e.key === "Tab" && !mod) {
                const info = s.drag?.info ?? s.drivers;
                const point =
                    s.selection?.kind === "point" ? s.selection.name : null;
                if (!info || (!dragging() && !point)) return;
                e.preventDefault();
                const axis = tabAxis(e, info.axis);
                if (dragging()) void cycleDrag(axis);
                else if (point) {
                    void cyclePreferred(point, axis).then((next) => {
                        if (next) useStore.getState().setDrivers(next);
                    });
                }
            } else if (mod && key === "z") {
                e.preventDefault();
                // A nudge run commits first, so undo takes it back whole.
                void endNudges().then(e.shiftKey ? redoEdit : undoEdit);
            } else if (mod && key === "y") {
                e.preventDefault();
                void endNudges().then(redoEdit);
            } else if (!mod && (e.key === "Delete" || e.key === "Backspace")) {
                if (!s.selection || dragging()) return;
                e.preventDefault();
                deleteSelection();
            } else if (e.key === "Escape") {
                // The innermost thing in progress first.
                if (dragging()) void endDrag(true);
                else if (s.relate) cancelRelate();
                else if (s.draft) endPath();
                else s.select(null);
            } else if (e.key === "Enter" && s.draft) {
                endPath();
            }
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, []);
}
