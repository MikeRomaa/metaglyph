// The source pane's Format button (and Shift+Alt+F): the engine formats
// the text as `mg fmt` does, and the result is applied as one undo step.

import type { ChangeSpec } from "@codemirror/state";
import { format } from "../engine/client.ts";
import { useStore } from "../state/store.ts";
import { formatText } from "./editor.ts";
import { whitespaceChanges } from "./whitespace.ts";

/** Formats the source pane's text. Says why in a notice when it can't. */
export async function formatSource() {
    const { text, setNotice } = useStore.getState();
    const result = await format(text);
    if (!result) return;
    if (result.status === "syntaxErrors") {
        setNotice("Fix the syntax errors before formatting.", "info");
        return;
    }
    if (result.status === "wouldLoseText") {
        setNotice(
            "Not formatting: a comment inside a `( … )` config can't be placed yet. Move it above the declaration.",
            "info",
        );
        return;
    }
    // Typing while the engine worked makes the result stale.
    if (useStore.getState().text !== text || result.text === text) return;
    const changes: ChangeSpec = whitespaceChanges(text, result.text) ?? {
        from: 0,
        to: text.length,
        insert: result.text,
    };
    formatText(changes);
}
