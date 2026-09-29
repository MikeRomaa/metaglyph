// Canvas ↔ source sync (plan 5, §2.2): the selected declaration is
// highlighted in the source, and moving the cursor into a declaration
// selects it on the canvas.

import type { Extension, Range } from "@codemirror/state";
import { RangeSet, StateEffect, StateField } from "@codemirror/state";
import type { DecorationSet } from "@codemirror/view";
import {
    Decoration,
    EditorView,
    GutterMarker,
    gutterLineClass,
} from "@codemirror/view";
import type { Span } from "../engine/types.ts";
import { pathKey, spanContains } from "../font/lookup.ts";
import type { Selection } from "../state/store.ts";
import { useStore } from "../state/store.ts";

export interface Highlight {
    /** The selected declaration. */
    selected: Span | null;
    /** The declaration the sheet is about (the active glyph). */
    context: Span | null;
}

export const setHighlight = StateEffect.define<Highlight>();

/** Adds a class to a line's gutter elements. */
class StripeMarker extends GutterMarker {
    override elementClass: string;

    constructor(elementClass: string) {
        super();
        this.elementClass = elementClass;
    }
    override eq(other: GutterMarker) {
        return other.elementClass === this.elementClass;
    }
}

const selectedStripe = new StripeMarker("cm-mg-selected-gutter");
const contextStripe = new StripeMarker("cm-mg-context-gutter");
const selectedLine = Decoration.line({ class: "cm-mg-selected" });

interface HighlightState {
    lines: DecorationSet;
    gutter: RangeSet<GutterMarker>;
}

function lineStarts(state: EditorView["state"], span: Span | null): number[] {
    if (!span) return [];
    const length = state.doc.length;
    const from = state.doc.lineAt(Math.min(span[0], length));
    const to = state.doc.lineAt(Math.min(span[1], length));
    const out: number[] = [];
    for (let n = from.number; n <= to.number; n++)
        out.push(state.doc.line(n).from);
    return out;
}

const highlightField = StateField.define<HighlightState>({
    create: () => ({ lines: Decoration.none, gutter: RangeSet.empty }),
    update(value, tr) {
        let next = {
            lines: value.lines.map(tr.changes),
            gutter: value.gutter.map(tr.changes),
        };
        for (const effect of tr.effects) {
            if (!effect.is(setHighlight)) continue;
            const selected = lineStarts(tr.state, effect.value.selected);
            const selectedSet = new Set(selected);
            const context = lineStarts(tr.state, effect.value.context).filter(
                (pos) => !selectedSet.has(pos),
            );
            const gutter: Range<GutterMarker>[] = [
                ...selected.map((pos) => selectedStripe.range(pos)),
                ...context.map((pos) => contextStripe.range(pos)),
            ].sort((a, b) => a.from - b.from);
            next = {
                lines: Decoration.set(
                    selected.map((pos) => selectedLine.range(pos)),
                ),
                gutter: RangeSet.of(gutter),
            };
        }
        return next;
    },
    provide: (field) => [
        EditorView.decorations.from(field, (v) => v.lines),
        gutterLineClass.from(field, (v) => v.gutter),
    ],
});

export const highlightTheme = EditorView.theme({
    ".cm-mg-selected": { background: "var(--hi)" },
    ".cm-lineNumbers .cm-gutterElement.cm-mg-selected-gutter": {
        borderLeftColor: "var(--acc)",
        background: "var(--hi)",
        color: "var(--ink)",
    },
    ".cm-lineNumbers .cm-gutterElement.cm-mg-context-gutter": {
        borderLeftColor: "var(--ink)",
    },
});

export function highlightExtension(): Extension {
    return [highlightField, highlightTheme];
}

/** The innermost declaration containing `offset`, as a selection; plus the
 * glyph or kern it belongs to. `null` when the store's spans don't match
 * the editor's text. */
export function selectionAt(offset: number): {
    selection: Selection | null;
    glyph?: string;
    kern?: number;
} | null {
    const s = useStore.getState();
    // The views' spans match the text only when it evaluated.
    if (!s.doc?.evaluated || s.doc.version !== s.version || !s.font)
        return null;

    const candidates: { sel: Omit<Selection, "origin">; kern?: number }[] = [];
    const add = (
        kind: Selection["kind"],
        name: string,
        span: Span,
        kern?: number,
    ) => {
        if (spanContains(span, offset))
            candidates.push({ sel: { kind, name, span }, kern });
    };

    const glyph = s.font.glyphs.find((g) => spanContains(g.span, offset));
    if (glyph && s.scene && glyph.name === s.scene.name) {
        for (const p of s.scene.points) add("point", p.name, p.span);
        for (const l of s.scene.lines) {
            // A `polar` ray shares its point's declaration.
            if (!l.of) add("line", l.name, l.span);
        }
        for (const m of s.scene.measures) add("measure", m.name, m.span);
        s.scene.components.forEach((c, i) => {
            add("component", `${i}`, c.span);
        });
        for (const path of s.scene.paths) {
            add("path", pathKey(path), path.span);
            path.segments.forEach((seg, i) => {
                add("segment", `${pathKey(path)}/${i}`, seg.span);
            });
        }
    }
    s.font.kerns.forEach((k, i) => {
        add("kern", `${i}`, k.span, i);
    });
    for (const m of s.font.metrics) add("metric", m.name, m.span);
    for (const l of s.font.lets) add("let", l.name, l.span);
    for (const g of s.font.groups) add("group", g.name, g.span);

    candidates.sort(
        (a, b) =>
            a.sel.span[1] - a.sel.span[0] - (b.sel.span[1] - b.sel.span[0]),
    );
    const best = candidates[0];
    return {
        selection: best ? { ...best.sel, origin: "source" } : null,
        glyph: glyph?.name,
        kern: best?.kern,
    };
}
