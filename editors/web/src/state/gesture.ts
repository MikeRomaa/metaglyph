// Edit gestures (plan 5, §1.6): a guide, metric or kern drag re-runs one
// op against the drag-start text on every move. The engine pins that text,
// so each result is the net change from the start; the text moves out of
// history and the drag commits as one undo step.

import { pin, runEdit, unpin } from "../engine/client.ts";
import type { Op } from "../engine/types.ts";
import { beginGesture, type EditorGesture } from "../source/editor.ts";
import { settled } from "./actions.ts";
import { useStore } from "./store.ts";

export class EditGesture {
    private readonly editor: EditorGesture;
    private readonly version: number;
    /** The newest op not yet run; a newer one replaces it. */
    private next: Op | null = null;
    private running: Promise<void> | null = null;
    private ended = false;
    private onResult?: (summary: string | null) => void;
    /** The last result's summary, for a live callout. */
    summary: string | null = null;

    private constructor(editor: EditorGesture, version: number) {
        this.editor = editor;
        this.version = version;
    }

    /**
     * Starts a gesture labelled `label` on the current text, once the
     * views reflect it (so spans and values read from them are the
     * start's). Null, with a notice, when the text can't be edited.
     */
    static async begin(label: string): Promise<EditGesture | null> {
        await settled();
        const s = useStore.getState();
        if (!s.doc?.parseOk) {
            s.setNotice("Read-only until the source parses again.");
            return null;
        }
        // A drag reads values from the views, which lag an unevaluated text.
        if (!s.doc.evaluated) {
            s.setNotice("Fix the source's errors to drag.");
            return null;
        }
        const version = s.version;
        if (!(await pin(version))) return null;
        const editor = beginGesture(label);
        if (!editor) {
            await unpin();
            return null;
        }
        return new EditGesture(editor, version);
    }

    /** Moves the text to the start text plus `op`'s change; `onResult`
     * gets the op's summary once applied. */
    update(op: Op, onResult?: (summary: string | null) => void) {
        if (this.ended) return;
        this.next = op;
        this.onResult = onResult;
        this.running ??= this.pump();
    }

    private async pump() {
        while (this.next && !this.ended) {
            const op = this.next;
            this.next = null;
            const result = await runEdit(op, this.version);
            if (this.ended) break;
            if (result.status === "ok") {
                this.editor.retargetSteps(result.steps);
                this.summary = result.summary ?? null;
                this.onResult?.(this.summary);
            } else if (result.status === "invalid") {
                useStore.getState().setNotice(result.message);
            }
        }
        this.running = null;
    }

    /** Commits the gesture as one undo step, or reverts it. */
    async end(cancel = false) {
        if (this.ended) return;
        // The last move's result first.
        while (this.running) await this.running;
        this.ended = true;
        if (cancel) this.editor.cancel();
        else this.editor.finish();
        await unpin();
    }
}
