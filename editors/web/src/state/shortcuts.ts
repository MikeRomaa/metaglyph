import { useEffect } from "react";
import { redoEdit, undoEdit } from "../source/editor.ts";
import { deleteSelection, endPath } from "./actions.ts";
import { useStore } from "./store.ts";

/** Whether a key event belongs to something the user is typing in. */
export function isTyping(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    return !!target?.closest("input, textarea, select, .cm-editor");
}

/** Edit shortcuts outside the source pane (it has its own): undo and redo
 * act on the one text history; Delete removes the selection. */
export function useEditShortcuts() {
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (isTyping(e)) return;
            const mod = e.ctrlKey || e.metaKey;
            const key = e.key.toLowerCase();
            if (mod && key === "z") {
                e.preventDefault();
                if (e.shiftKey) redoEdit();
                else undoEdit();
            } else if (mod && key === "y") {
                e.preventDefault();
                redoEdit();
            } else if (!mod && (e.key === "Delete" || e.key === "Backspace")) {
                if (!useStore.getState().selection) return;
                e.preventDefault();
                deleteSelection();
            } else if (e.key === "Escape" || e.key === "Enter") {
                // Ends a path in progress first; then clears the selection.
                const s = useStore.getState();
                if (s.draft) endPath();
                else if (e.key === "Escape") s.select(null);
            }
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, []);
}
