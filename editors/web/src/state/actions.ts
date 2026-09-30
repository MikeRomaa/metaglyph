// Canvas edits (plan 5, §1.1): an op goes to the engine, which returns the
// text changes; they are applied here as one transaction.

import { runEdit } from "../engine/client.ts";
import type {
    Created,
    EditResult,
    FieldValue,
    Op,
    PathInfo,
    Pt,
    SegmentKind,
    Span,
} from "../engine/types.ts";
import { pathKey } from "../font/lookup.ts";
import { applyChanges } from "../source/editor.ts";
import type { SelectionKind } from "./store.ts";
import { useStore } from "./store.ts";

/** Resolves once the views reflect the current text, so spans read from
 * them match what the engine will edit. */
export function settled(): Promise<void> {
    const ready = () => {
        const s = useStore.getState();
        return s.doc?.version === s.version;
    };
    if (ready()) return Promise.resolve();
    return new Promise((resolve) => {
        const unsubscribe = useStore.subscribe(() => {
            if (ready()) {
                unsubscribe();
                resolve();
            }
        });
    });
}

/**
 * Runs the op `build` makes and applies its changes as one undo step
 * labelled `label`. `build` runs once the views are current, so the spans
 * it reads are for the text being edited; returning `null` cancels.
 * Explains a refusal in the status bar, and returns the result so a
 * caller can show it inline too. A declaration the op creates is selected
 * once it appears, with its name field open (plan 5, §1.3).
 */
export async function performEdit(
    label: string,
    build: () => Op | null,
    options: { rename?: boolean } = {},
): Promise<EditResult | null> {
    for (let attempt = 0; attempt < 3; attempt++) {
        await settled();
        const op = build();
        if (!op) return null;
        const store = useStore.getState();
        const result = await runEdit(op, store.version);
        // The text moved on while the op ran: try again against it.
        if (result.status === "stale") continue;
        if (result.status === "ok") {
            if (useStore.getState().version !== result.version) continue;
            if (result.created) {
                const [kind, name] = selectionFor(result.created);
                store.setPending(kind, name, options.rename ?? true);
            }
            if (result.steps.length > 0) applyChanges(result.steps, label);
            return result;
        }
        if (result.status === "readOnly") {
            store.setNotice("Read-only until the source parses again.");
        } else {
            store.setNotice(result.message);
        }
        return result;
    }
    useStore.getState().setNotice("The source kept changing; try again.");
    return null;
}

/** How a created declaration is selected. */
function selectionFor(created: Created): [SelectionKind, string] {
    switch (created.kind) {
        case "let":
            return ["measure", created.name];
        case "component": {
            // Appended last: its index is the current count.
            const count = useStore.getState().scene?.components.length ?? 0;
            return ["component", String(count)];
        }
        default:
            return [created.kind, created.name];
    }
}

/** Renames the selected declaration; the selection follows it. */
export async function renameSelection(
    name: string,
): Promise<EditResult | null> {
    const selection = useStore.getState().selection;
    if (!selection) return null;
    const result = await performEdit("rename", () => {
        const current = useStore.getState().selection;
        return current ? { op: "rename", span: current.span, name } : null;
    });
    if (result?.status === "ok" && result.steps.length > 0) {
        const s = useStore.getState();
        // Segments, components and kerns are selected by position, not
        // name; everything else by its new name.
        const positional = ["segment", "component", "kern"];
        if (s.selection && !positional.includes(s.selection.kind)) {
            s.select({ ...s.selection, name });
        }
    }
    return result;
}

/** Deletes the selected declaration (plan 5: references to it become
 * diagnostics, as intended). */
export function deleteSelection() {
    void performEdit("delete", () => {
        const current = useStore.getState().selection;
        return current ? { op: "delete", span: current.span } : null;
    });
}

// ---------------------------------------------------------------------
// Construction tools (plan 5, §1.4)

function glyph(): string | null {
    return useStore.getState().glyph;
}

export function addPoint(at: Pt) {
    void performEdit("add_point", () => {
        const g = glyph();
        return g ? { op: "addPoint", glyph: g, at } : null;
    });
}

/** A horizontal guide through `at`, or a vertical one. */
export function addGuide(at: Pt, vertical: boolean) {
    void performEdit("add_guide", () => {
        const g = glyph();
        if (!g) return null;
        return {
            op: "addLine",
            glyph: g,
            line: vertical
                ? { kind: "vline", x: at[0] }
                : { kind: "hline", y: at[1] },
        };
    });
}

export function addLineThrough(a: string, b: string) {
    void performEdit("add_line", () => {
        const g = glyph();
        return g
            ? { op: "addLine", glyph: g, line: { kind: "through", a, b } }
            : null;
    });
}

export function addMeasure(a: string, b: string) {
    void performEdit("add_measure", () => {
        const g = glyph();
        return g ? { op: "addMeasure", glyph: g, a, b } : null;
    });
}

export function placeComponent(target: string, offset: Pt) {
    void performEdit(
        "add_component",
        () => {
            const g = glyph();
            return g ? { op: "addComponent", glyph: g, target, offset } : null;
        },
        { rename: false },
    );
}

// ---------------------------------------------------------------------
// Path tool (plan 5, §1.4)

/** One path-tool click at `at`: starts a path, adds a `line` to it, or —
 * when `handle` (where the pointer was released after dragging) is given
 * — a `cube` whose end tangent follows the drag. Clicking the start point
 * closes the path. */
export async function pathClick(at: Pt, handle: Pt | null, onStart: boolean) {
    const s = useStore.getState();
    const g = s.glyph;
    if (!g) return;
    const draft = s.draft;

    if (!draft) {
        const result = await performEdit(
            "path_start",
            () => ({
                op: "pathStart",
                glyph: g,
                at,
                copyFrom: useStore.getState().lastPath ?? undefined,
            }),
            { rename: false },
        );
        if (result?.status === "ok" && result.created) {
            useStore
                .getState()
                .setDraft({ path: result.created.name, start: at, last: at });
            useStore.getState().setLastPath(result.created.name);
        }
        return;
    }

    if (onStart) {
        await performEdit("path_close", () => ({
            op: "pathClose",
            glyph: g,
            path: draft.path,
        }));
        useStore.getState().setDraft(null);
        return;
    }

    let c1: Pt | undefined;
    let c2: Pt | undefined;
    if (handle) {
        // The drag sets the outgoing handle at `at`; the incoming one
        // mirrors it, and the first control sits a third of the way out.
        c2 = [2 * at[0] - handle[0], 2 * at[1] - handle[1]];
        c1 = [
            draft.last[0] + (at[0] - draft.last[0]) / 3,
            draft.last[1] + (at[1] - draft.last[1]) / 3,
        ];
    }
    const result = await performEdit(
        handle ? "path_curve" : "path_line",
        () => ({ op: "pathAppend", glyph: g, path: draft.path, at, c1, c2 }),
        { rename: false },
    );
    if (result?.status === "ok") {
        useStore.getState().setDraft({ ...draft, last: at });
    }
}

export function endPath() {
    useStore.getState().setDraft(null);
}

// ---------------------------------------------------------------------
// Path and segment properties (plan 5, §1.4)

function remember(path: PathInfo) {
    if (path.name) useStore.getState().setLastPath(path.name);
}

/** A path's span in the current views, found by its key (the span given
 * at click time may have moved). */
function pathSpan(path: PathInfo): Span | null {
    const key = pathKey(path);
    return (
        useStore.getState().scene?.paths.find((p) => pathKey(p) === key)
            ?.span ?? null
    );
}

/** Segment `index` of `path`'s span in the current views. */
function segmentSpan(path: PathInfo, index: number): Span | null {
    const key = pathKey(path);
    const current = useStore
        .getState()
        .scene?.paths.find((p) => pathKey(p) === key);
    return current?.segments[index]?.span ?? null;
}

export function setPathField(path: PathInfo, name: string, value: FieldValue) {
    remember(path);
    return performEdit(`set_${name}`, () => {
        const span = pathSpan(path);
        return span ? { op: "setField", span, name, value } : null;
    });
}

export function removePathField(path: PathInfo, name: string) {
    remember(path);
    return performEdit(`remove_${name}`, () => {
        const span = pathSpan(path);
        return span ? { op: "removeField", span, name } : null;
    });
}

export function setFill(path: PathInfo, on: boolean) {
    remember(path);
    return performEdit("set_fill", () => {
        const span = pathSpan(path);
        return span ? { op: "setFill", span, on } : null;
    });
}

/** Sets a field of segment `index` of `path`. */
export function setSegmentField(
    path: PathInfo,
    index: number,
    name: string,
    value: FieldValue,
) {
    return performEdit(`set_${name}`, () => {
        const span = segmentSpan(path, index);
        return span ? { op: "setField", span, name, value } : null;
    });
}

/**
 * Converts a segment to `kind` (plan 5, §1.4), seeding the new control
 * points from the current shape: a quad's control at the chord midpoint
 * (or the average of a cube's), a cube's at a third and two thirds of the
 * chord (a quad's by exact degree elevation), an arc's centre at the
 * chord midpoint.
 */
export function setSegmentKind(
    path: PathInfo,
    index: number,
    kind: SegmentKind,
) {
    const segment = path.segments[index];
    const from = path.segments[index - 1]?.to;
    const to = segment?.to;
    if (!segment || !from) return;
    if (!to || kind === segment.kind) return;
    const lerp = (a: Pt, b: Pt, t: number): Pt => [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
    ];
    let controls: Pt[] = [];
    if (kind === "quad") {
        controls =
            segment.kind === "cube" && segment.controls.length === 2
                ? [lerp(segment.controls[0], segment.controls[1], 0.5)]
                : [lerp(from, to, 0.5)];
    } else if (kind === "cube") {
        controls =
            segment.kind === "quad" && segment.controls.length === 1
                ? [
                      lerp(from, segment.controls[0], 2 / 3),
                      lerp(to, segment.controls[0], 2 / 3),
                  ]
                : [lerp(from, to, 1 / 3), lerp(from, to, 2 / 3)];
    } else if (kind === "arc") {
        controls = [lerp(from, to, 0.5)];
    }
    void performEdit("set_segment_kind", () => {
        const span = segmentSpan(path, index);
        return span ? { op: "setSegmentKind", span, kind, controls } : null;
    });
}
