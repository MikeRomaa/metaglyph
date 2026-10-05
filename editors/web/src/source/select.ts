// Ctrl+D as plain text search: CodeMirror's own `selectNextOccurrence`
// matches whole words once the selection came from a bare cursor, so
// `stem` skips the `stem` in `stem0`. This matches the selected text
// anywhere, case-sensitively, like a find.

import { EditorSelection, type EditorState } from "@codemirror/state";
import type { Command } from "@codemirror/view";

/**
 * The selection after one Ctrl+D: from bare cursors, the word under the
 * main one; otherwise every range kept, plus the next place the main
 * range's text occurs after it (wrapping round the document, skipping
 * what is already selected). Null when there is nothing to add.
 */
export function nextMatch(state: EditorState): EditorSelection | null {
    const { selection } = state;
    const main = selection.main;
    if (selection.ranges.every((r) => r.empty)) {
        const word = state.wordAt(main.head);
        return word ? EditorSelection.single(word.from, word.to) : null;
    }
    const needle = state.sliceDoc(main.from, main.to);
    if (!needle) return null;
    const text = state.doc.toString();
    const taken = (from: number) =>
        selection.ranges.some(
            (r) => r.from === from && r.to === from + needle.length,
        );
    // From the main range's end to the end, then from the start back to it.
    for (const [start, stop] of [
        [main.to, text.length],
        [0, main.from],
    ]) {
        let at = text.indexOf(needle, start);
        while (at >= 0 && at < stop) {
            if (!taken(at)) {
                return selection.addRange(
                    EditorSelection.range(at, at + needle.length),
                );
            }
            at = text.indexOf(needle, at + 1);
        }
    }
    return null;
}

export const selectNextMatch: Command = (view) => {
    const next = nextMatch(view.state);
    if (!next) return false;
    view.dispatch({
        selection: next,
        scrollIntoView: true,
        userEvent: "select.search",
    });
    return true;
};
