import { history, redo, undo, undoDepth } from "@codemirror/commands";
import { EditorState, type TransactionSpec } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { composeSteps, editSpec, Gesture } from "./history.ts";

describe("composeSteps", () => {
    it("applies steps each against the text the previous made", () => {
        const doc = "glyph A {\n}\n";
        // Step 1 inserts a let; step 2's offsets already include it.
        const steps = [
            [{ from: 9, to: 9, insert: "\n    let p0 = (1, 2);" }],
            [{ from: 30, to: 30, insert: "\n    path p {}" }],
        ];
        const set = composeSteps(doc.length, steps);
        const state = EditorState.create({ doc });
        expect(state.update({ changes: set }).state.doc.toString()).toBe(
            "glyph A {\n    let p0 = (1, 2);\n    path p {}\n}\n",
        );
    });
});

function start(doc: string) {
    let state = EditorState.create({ doc, extensions: [history()] });
    const apply = (spec: TransactionSpec) => {
        state = state.update(spec).state;
    };
    const run = (command: typeof undo) =>
        command({
            state,
            dispatch: (tr) => {
                state = tr.state;
            },
        });
    return { get: () => state, apply, run };
}

describe("editSpec", () => {
    it("makes each edit its own undo step, even back to back", () => {
        const doc = start("let a = 1;");
        doc.apply(editSpec({ from: 8, to: 9, insert: "2" }, "set"));
        doc.apply(editSpec({ from: 8, to: 9, insert: "3" }, "set"));
        expect(undoDepth(doc.get())).toBe(2);
        doc.run(undo);
        expect(doc.get().doc.toString()).toBe("let a = 2;");
    });

    it("labels the transaction", () => {
        const state = EditorState.create({ doc: "x" });
        const tr = state.update(editSpec({ from: 0, insert: "y" }, "rename"));
        expect(tr.isUserEvent("mg.rename")).toBe(true);
    });
});

describe("Gesture", () => {
    it("coalesces a drag into one undo step", () => {
        const doc = start("let a = (0.500 * w, h);");
        const gesture = new Gesture(doc.get(), "drag");
        for (const value of ["0.510", "0.525", "0.561"]) {
            const text = doc.get().doc.toString();
            const from = text.indexOf("0.5");
            doc.apply(
                gesture.step(doc.get(), { from, to: from + 5, insert: value }),
            );
        }
        for (const spec of gesture.finish()) doc.apply(spec);

        expect(doc.get().doc.toString()).toBe("let a = (0.561 * w, h);");
        expect(undoDepth(doc.get())).toBe(1);
        doc.run(undo);
        expect(doc.get().doc.toString()).toBe("let a = (0.500 * w, h);");
        doc.run(redo);
        expect(doc.get().doc.toString()).toBe("let a = (0.561 * w, h);");
    });

    it("leaves no history for a gesture that changed nothing", () => {
        const doc = start("let a = 1;");
        const gesture = new Gesture(doc.get(), "drag");
        expect(gesture.finish()).toEqual([]);
        expect(undoDepth(doc.get())).toBe(0);
    });

    it("stays separate from typing before it", () => {
        const doc = start("let a = 1;");
        doc.apply({
            changes: { from: 10, insert: " " },
            userEvent: "input.type",
        });
        const gesture = new Gesture(doc.get(), "drag");
        doc.apply(gesture.step(doc.get(), { from: 8, to: 9, insert: "5" }));
        for (const spec of gesture.finish()) doc.apply(spec);
        expect(undoDepth(doc.get())).toBe(2);
        doc.run(undo);
        expect(doc.get().doc.toString()).toBe("let a = 1; ");
    });
});
