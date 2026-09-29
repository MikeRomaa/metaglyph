// Canvas edits in the one text history (plan 5, "Undo"): every edit is a
// transaction and one undo step; a gesture (a drag, a slider) coalesces
// into one step however many transactions it dispatches.
//
// CodeMirror only joins typing and deletes into one history event, so a
// gesture keeps its intermediate steps out of history and, when it ends,
// replays its net change as a single event.

import { isolateHistory } from "@codemirror/commands";
import type {
    ChangeSpec,
    EditorState,
    Text,
    TransactionSpec,
} from "@codemirror/state";
import { ChangeSet, Transaction } from "@codemirror/state";

/** One change set from an op's sequential steps: each step's offsets are
 * in the text after the steps before it. */
export function composeSteps(
    length: number,
    steps: { from: number; to: number; insert: string }[][],
): ChangeSet {
    let set = ChangeSet.empty(length);
    for (const step of steps) {
        set = set.compose(ChangeSet.of(step, set.newLength));
    }
    return set;
}

/** One edit as its own undo step, labelled `mg.<label>`. */
export function editSpec(changes: ChangeSpec, label: string): TransactionSpec {
    return {
        changes,
        userEvent: `mg.${label}`,
        annotations: isolateHistory.of("full"),
    };
}

/** A gesture in progress: `step` for each change as it happens, `finish`
 * at the end. */
export class Gesture {
    private readonly startDoc: Text;
    private readonly label: string;
    private net: ChangeSet;

    constructor(state: EditorState, label: string) {
        this.startDoc = state.doc;
        this.label = label;
        this.net = ChangeSet.empty(state.doc.length);
    }

    /** A step: applied now, kept out of history. */
    step(state: EditorState, changes: ChangeSpec): TransactionSpec {
        const set = state.changes(changes);
        this.net = this.net.compose(set);
        return {
            changes: set,
            userEvent: `mg.${this.label}`,
            annotations: Transaction.addToHistory.of(false),
        };
    }

    /** A step given as the net change from the gesture's start (a drag
     * solves against the drag-start text): applied now, out of history. */
    retarget(changes: ChangeSpec): TransactionSpec {
        const next = ChangeSet.of(changes, this.startDoc.length);
        const step = this.net.invert(this.startDoc).compose(next);
        this.net = next;
        return {
            changes: step,
            userEvent: `mg.${this.label}`,
            annotations: Transaction.addToHistory.of(false),
        };
    }

    /** Back to the start, out of history (a cancelled drag). */
    cancel(): TransactionSpec {
        return this.retarget([]);
    }

    /** Undo the steps outside history, then redo their net change as one
     * undo step. Dispatch both, in order; empty if nothing changed. */
    finish(): TransactionSpec[] {
        if (this.net.empty) return [];
        return [
            {
                changes: this.net.invert(this.startDoc),
                annotations: Transaction.addToHistory.of(false),
            },
            editSpec(this.net, this.label),
        ];
    }
}
