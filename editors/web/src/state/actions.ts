// Canvas edits (plan 5, §1.1): an op goes to the engine, which returns the
// text changes; they are applied here as one transaction.

import { runEdit } from "../engine/client.ts";
import type { EditResult, Op } from "../engine/types.ts";
import { applyChanges } from "../source/editor.ts";
import { useStore } from "./store.ts";

/** Resolves once the views reflect the current text, so spans read from
 * them match what the engine will edit. */
function settled(): Promise<void> {
    const ready = () => {
        const s = useStore.getState();
        return s.doc?.version === s.version;
    };
    if (ready()) return Promise.resolve();
    return new Promise((resolve) => {
        const unsubscribe = useStore.subscribe(() => {
            if (ready()) {
                unsubscribe();
                resolve();
            }
        });
    });
}

/**
 * Runs the op `build` makes and applies its changes as one undo step
 * labelled `label`. `build` runs once the views are current, so the spans
 * it reads are for the text being edited; returning `null` cancels.
 * Explains a refusal in the status bar, and returns the result so a
 * caller can show it inline too.
 */
export async function performEdit(
    label: string,
    build: () => Op | null,
): Promise<EditResult | null> {
    for (let attempt = 0; attempt < 3; attempt++) {
        await settled();
        const op = build();
        if (!op) return null;
        const store = useStore.getState();
        const result = await runEdit(op, store.version);
        // The text moved on while the op ran: try again against it.
        if (result.status === "stale") continue;
        if (result.status === "ok") {
            if (useStore.getState().version !== result.version) continue;
            if (result.changes.length > 0) applyChanges(result.changes, label);
            return result;
        }
        if (result.status === "readOnly") {
            store.setNotice("Read-only until the source parses again.");
        } else {
            store.setNotice(result.message);
        }
        return result;
    }
    useStore.getState().setNotice("The source kept changing; try again.");
    return null;
}

/** Renames the selected declaration; the selection follows it. */
export async function renameSelection(
    name: string,
): Promise<EditResult | null> {
    const selection = useStore.getState().selection;
    if (!selection) return null;
    const result = await performEdit("rename", () => {
        const current = useStore.getState().selection;
        return current ? { op: "rename", span: current.span, name } : null;
    });
    if (result?.status === "ok" && result.changes.length > 0) {
        const s = useStore.getState();
        // Segments, components and kerns are selected by position, not
        // name; everything else by its new name.
        const positional = ["segment", "component", "kern"];
        if (s.selection && !positional.includes(s.selection.kind)) {
            s.select({ ...s.selection, name });
        }
    }
    return result;
}

/** Deletes the selected declaration (plan 5: references to it become
 * diagnostics, as intended). */
export function deleteSelection() {
    void performEdit("delete", () => {
        const current = useStore.getState().selection;
        return current ? { op: "delete", span: current.span } : null;
    });
}
