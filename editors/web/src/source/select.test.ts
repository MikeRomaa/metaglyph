import { EditorSelection, EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { nextMatch } from "./select.ts";

/** `doc` with the cursor (or selection) given, as a state. */
function state(doc: string, from: number, to = from) {
    return EditorState.create({
        doc,
        selection: EditorSelection.single(from, to),
        extensions: EditorState.allowMultipleSelections.of(true),
    });
}

/** Applies Ctrl+D `times` times; the selected ranges after. */
function press(s: EditorState, times: number) {
    let current = s;
    for (let i = 0; i < times; i++) {
        const next = nextMatch(current);
        if (!next) break;
        current = current.update({ selection: next }).state;
    }
    return current.selection.ranges.map((r) => [r.from, r.to]);
}

describe("Ctrl+D", () => {
    const doc = "path stem\nlet stem0 = 1;\nlet upright_stem = stem0;";

    it("selects the word under a bare cursor first", () => {
        expect(press(state(doc, 6), 1)).toEqual([[5, 9]]);
    });

    it("then matches the text anywhere, not just whole words", () => {
        const found = press(state(doc, 6), 4).map(([from, to]) =>
            doc.slice(from, to),
        );
        expect(found).toEqual(["stem", "stem", "stem", "stem"]);
        const starts = press(state(doc, 6), 4).map(([from]) => from);
        // `stem`, then inside `stem0`, `upright_stem`, and the last `stem0`:
        // every occurrence, in document order.
        const every = [...doc.matchAll(/stem/g)].map((m) => m.index);
        expect(every).toHaveLength(4);
        expect(starts).toEqual(every);
    });

    it("wraps round and stops once every match is selected", () => {
        const s = state(
            doc,
            doc.lastIndexOf("stem"),
            doc.lastIndexOf("stem") + 4,
        );
        expect(press(s, 10)).toHaveLength(4);
    });

    it("is case-sensitive", () => {
        expect(press(state("Stem stem", 5, 9), 3)).toEqual([[5, 9]]);
    });
});
