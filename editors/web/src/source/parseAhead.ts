// Keeps the whole document parsed, so highlighting is already there when
// you scroll. CodeMirror parses a StreamLanguage around the viewport and
// fills in the rest during idle time, which shows as a delay before
// far-off text is highlighted. A full parse of a 1,200-line font takes
// about 8 ms, so it is simply done up front, and again after edits.

import { forceParsing, syntaxTreeAvailable } from "@codemirror/language";
import type { EditorView, PluginValue, ViewUpdate } from "@codemirror/view";
import { ViewPlugin } from "@codemirror/view";

/** How long typing must pause before re-parsing to the end. */
const AFTER_EDIT_MS = 150;
/** The most one parse may take; the rest waits for CodeMirror's own. */
const BUDGET_MS = 100;

class ParseAhead implements PluginValue {
    readonly view: EditorView;
    timer: ReturnType<typeof setTimeout> | undefined;

    constructor(view: EditorView) {
        this.view = view;
        this.schedule(0);
    }

    update(update: ViewUpdate) {
        if (update.docChanged) this.schedule(AFTER_EDIT_MS);
        // Scrolled past what is parsed (mid-edit, before the timer): now.
        else if (
            update.viewportChanged &&
            !syntaxTreeAvailable(update.state, update.view.viewport.to)
        ) {
            this.schedule(0);
        }
    }

    /** Parses to the end outside the update cycle, where `forceParsing`
     * may dispatch. */
    schedule(delay: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => {
            const { view } = this;
            forceParsing(view, view.state.doc.length, BUDGET_MS);
        }, delay);
    }

    destroy() {
        clearTimeout(this.timer);
    }
}

export const parseAhead = ViewPlugin.fromClass(ParseAhead);
