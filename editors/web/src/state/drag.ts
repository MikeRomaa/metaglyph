// Point drags (plan 5, §1.5): the engine solves each pointer position for
// one literal; each step moves the text as a gesture, committed as one undo
// step when the drag ends.

import {
    dragBegin,
    dragCycle,
    dragEnd,
    dragSet,
    dragTo,
} from "../engine/client.ts";
import {
    type DragInfo,
    type DragStep,
    lockText,
    type Pt,
} from "../engine/types.ts";
import { beginGesture, type EditorGesture } from "../source/editor.ts";
import { settled } from "./actions.ts";
import { useStore } from "./store.ts";

interface Active {
    gesture: EditorGesture;
    glyph: string;
    target: string;
    /** The latest pointer position, to re-solve after cycling drivers. */
    last: Pt | null;
}

let active: Active | null = null;

export function dragging() {
    return active !== null;
}

/** Why a point can't be dragged, from its drivers: every axis is locked. */
function lockedMessage(info: DragInfo) {
    return `${info.target} is set by ${lockText(info, 0)}: a drag never moves other points or top-level values.`;
}

/** Starts dragging point `target`; false if it can't be dragged. */
export async function startDrag(target: string): Promise<boolean> {
    if (active) return false;
    await settled();
    const s = useStore.getState();
    if (!s.instance || !s.glyph || !s.doc?.evaluated) {
        s.setNotice("Fix the source's errors to drag.");
        return false;
    }
    const info = await dragBegin(s.instance, s.glyph, target, s.version);
    if (!info) {
        s.setNotice(`${target} can't be dragged right now.`);
        return false;
    }
    if (info.axis[0] === null && info.axis[1] === null) {
        await dragEnd(s.glyph);
        s.setNotice(lockedMessage(info), "info");
        return false;
    }
    const gesture = beginGesture("drag");
    if (!gesture) {
        await dragEnd(s.glyph);
        return false;
    }
    active = { gesture, glyph: s.glyph, target, last: null };
    s.setDrag({ target, info, step: null });
    return true;
}

function apply(a: Active, step: DragStep | null) {
    if (!step || active !== a) return;
    a.gesture.retarget(step.changes);
    const drag = useStore.getState().drag;
    if (drag) useStore.getState().setDrag({ ...drag, step });
}

/** Solves for the pointer at `at` (font units). */
export function moveDrag(at: Pt) {
    const a = active;
    if (!a) return;
    a.last = at;
    void dragTo(at).then((step) => apply(a, step));
}

/** Sets the dragged driver `driver` to `value` (the scrub slider). */
export function scrubDrag(driver: number, value: number) {
    const a = active;
    if (!a) return;
    void dragSet(driver, value).then((step) => apply(a, step));
}

/** Moves axis `axis` (0: x, 1: y) to its next driver, and re-solves. */
export async function cycleDrag(axis: 0 | 1) {
    const a = active;
    if (!a) return;
    const info = await dragCycle(axis);
    const drag = useStore.getState().drag;
    if (info && drag && active === a) {
        useStore.getState().setDrag({ ...drag, info });
        if (a.last) moveDrag(a.last);
    }
}

/** Ends the drag: commits it as one undo step, or reverts it. */
export async function endDrag(cancel = false) {
    const a = active;
    if (!a) return;
    // The last pointer position's step, before committing.
    if (!cancel && a.last) apply(a, await dragTo(a.last));
    active = null;
    if (cancel) a.gesture.cancel();
    else a.gesture.finish();
    await dragEnd(a.glyph);
    useStore.getState().setDrag(null);
}

/** Cycles a point's preferred driver outside a drag (the DRIVERS panel's
 * Tab), returning its drivers with the new choice. */
export async function cyclePreferred(
    target: string,
    axis: 0 | 1,
): Promise<DragInfo | null> {
    if (active) return null;
    await settled();
    const s = useStore.getState();
    if (!s.instance || !s.glyph) return null;
    const started = await dragBegin(s.instance, s.glyph, target, s.version);
    if (!started) return null;
    const info = await dragCycle(axis);
    await dragEnd(s.glyph);
    return info;
}
