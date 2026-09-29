// The one CodeMirror view, for code outside the source pane that edits the
// text: canvas edits, and undo/redo from the canvas.

import { redo, undo } from "@codemirror/commands";
import type { EditorView } from "@codemirror/view";
import type { Change } from "../engine/types.ts";
import { flashEffect } from "./flash.ts";
import { editSpec } from "./history.ts";

let current: EditorView | null = null;

export function setEditorView(view: EditorView | null) {
    current = view;
}

/** Applies an edit op's changes as one transaction and one undo step,
 * flashing the text it inserted. */
export function applyChanges(changes: Change[], label: string) {
    const view = current;
    if (!view) return;
    const set = view.state.changes(changes);
    view.dispatch({
        ...editSpec(set, label),
        effects: flashEffect(view, set),
    });
}

export function undoEdit() {
    if (current) undo(current);
}

export function redoEdit() {
    if (current) redo(current);
}
