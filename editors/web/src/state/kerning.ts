// Kerning sheet edits (plan 5, §1.6 "Kerning"). Nudges and drags
// add-constant on the pair's `by`; the engine refuses pairs and group
// members that break spec §12.2, and the refusal is shown where the edit
// was made.

import type { EditResult, KernSideSpec, Span } from "../engine/types.ts";
import { performEdit } from "./actions.ts";
import { EditGesture } from "./gesture.ts";
import { useStore } from "./store.ts";

/** A run of nudges on one pair: one gesture, so the run is one undo
 * step. It ends once the nudges pause, or on any other edit. */
interface Run {
    index: number;
    total: number;
    /** The pair's span in the run's start text. */
    span: Span | null;
    gesture: Promise<EditGesture | null>;
    timer?: ReturnType<typeof setTimeout>;
}

let run: Run | null = null;

/** How long nudges may pause and still join the same undo step. */
const PAUSE_MS = 800;

/** Nudges kern `index` by `delta` raw units. */
export function nudgeKern(index: number, delta: number) {
    if (run && run.index !== index) void endNudges();
    if (!run) {
        const r: Run = {
            index,
            total: 0,
            span: null,
            gesture: EditGesture.begin("nudge_kern").then((g) => {
                // Read once the views show the start text.
                r.span = useStore.getState().font?.kerns[index]?.span ?? null;
                if (!g && run === r) run = null;
                return g;
            }),
        };
        run = r;
    }
    const r = run;
    r.total += delta;
    clearTimeout(r.timer);
    r.timer = setTimeout(() => void endNudges(), PAUSE_MS);
    void r.gesture.then((g) => {
        const em = useStore.getState().font?.em ?? 1000;
        if (!g || !r.span) return;
        g.update({
            op: "addConstant",
            span: r.span,
            name: "by",
            delta: r.total,
            em,
        });
    });
}

/** Commits a nudge run in progress. */
export async function endNudges() {
    const r = run;
    if (!r) return;
    run = null;
    clearTimeout(r.timer);
    await (await r.gesture)?.end();
}

/** A drag of the pair's right glyph: `by` follows the pointer. */
export class KernDrag {
    private readonly gesture: Promise<EditGesture | null>;
    private span: Span | null = null;

    constructor(index: number) {
        this.gesture = endNudges()
            .then(() => EditGesture.begin("drag_kern"))
            .then((g) => {
                this.span =
                    useStore.getState().font?.kerns[index]?.span ?? null;
                return g;
            });
    }

    /** `by` moved by `delta` raw units from the drag's start. */
    move(delta: number) {
        void this.gesture.then((g) => {
            const em = useStore.getState().font?.em ?? 1000;
            if (!g || !this.span) return;
            g.update({
                op: "addConstant",
                span: this.span,
                name: "by",
                delta,
                em,
            });
        });
    }

    async end(cancel = false) {
        await (await this.gesture)?.end(cancel);
    }
}

/** Converts kern `index`'s `by` literal to em units, or back to raw. */
export function setKernUnit(index: number, em: boolean) {
    return performEdit(em ? "kern_to_em" : "kern_to_raw", () => {
        const font = useStore.getState().font;
        const kern = font?.kerns[index];
        if (!font || !kern) return null;
        return { op: "kernUnit", span: kern.span, em, fontEm: font.em };
    });
}

/** A new pair; either side may declare a new group (one undo step). */
export async function newKern(
    left: KernSideSpec,
    right: KernSideSpec,
): Promise<EditResult | null> {
    await endNudges();
    return performEdit(
        "new_kern",
        () => ({ op: "newKern", left, right, by: 0 }),
        { rename: false },
    );
}

/** Adds `glyphs` to `group`, or removes them (one undo step). */
export function groupMember(group: string, glyphs: string[], add: boolean) {
    return performEdit(add ? "add_members" : "remove_member", () => ({
        op: "groupMember",
        group,
        glyphs,
        add,
    }));
}

/** Deletes kern `index` (as Delete does on a selected pair). */
export async function deleteKern(index: number) {
    await endNudges();
    return performEdit("delete_kern", () => {
        const kern = useStore.getState().font?.kerns[index];
        return kern ? { op: "delete", span: kern.span } : null;
    });
}

/** Deletes group `name` and the pairs that use it. */
export async function deleteGroup(name: string) {
    await endNudges();
    return performEdit("delete_group", () => ({ op: "deleteGroup", name }));
}
