// The one CodeMirror view, for code outside the source pane that edits the
// text: canvas edits, and undo/redo from the canvas.

import { redo, undo } from "@codemirror/commands";
import type { ChangeSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Change } from "../engine/types.ts";
import { flashEffect } from "./flash.ts";
import { composeSteps, editSpec, Gesture } from "./history.ts";

let current: EditorView | null = null;

export function setEditorView(view: EditorView | null) {
    current = view;
}

/** Applies an edit op's steps as one transaction and one undo step,
 * flashing the text it inserted. */
export function applyChanges(steps: Change[][], label: string) {
    const view = current;
    if (!view) return;
    const set = composeSteps(view.state.doc.length, steps);
    view.dispatch({
        ...editSpec(set, label),
        effects: flashEffect(view, set),
    });
}

/** Applies the formatter's changes as one undo step, `mg.format`. Not
 * flashed: formatting touches whitespace all over. */
export function formatText(changes: ChangeSpec) {
    current?.dispatch(editSpec(changes, "format"));
}

/** A gesture on the editor: steps are applied out of history as they come,
 * and `finish` commits them as one undo step (plan 5, "Undo"). */
export interface EditorGesture {
    /** Moves the text to the drag-start text plus `changes`. */
    retarget(changes: Change[]): void;
    /** `retarget` with an op's sequential steps, each in the text after
     * the steps before it. */
    retargetSteps(steps: Change[][]): void;
    cancel(): void;
    finish(): void;
}

export function beginGesture(label: string): EditorGesture | null {
    const view = current;
    if (!view) return null;
    const gesture = new Gesture(view.state, label);
    const length = view.state.doc.length;
    return {
        retarget: (changes) => view.dispatch(gesture.retarget(changes)),
        retargetSteps: (steps) =>
            view.dispatch(gesture.retarget(composeSteps(length, steps))),
        cancel: () => view.dispatch(gesture.cancel()),
        finish: () => {
            const specs = gesture.finish();
            specs.forEach((spec, i) => {
                // Flash the committed change, not the revert before it.
                if (i === specs.length - 1 && spec.changes) {
                    const set = view.state.changes(spec.changes);
                    view.dispatch({
                        ...spec,
                        changes: set,
                        effects: flashEffect(view, set),
                    });
                } else {
                    view.dispatch(spec);
                }
            });
        },
    };
}

export function undoEdit() {
    if (current) undo(current);
}

export function redoEdit() {
    if (current) redo(current);
}
