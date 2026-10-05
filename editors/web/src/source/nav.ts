// Quick navigation beside the scrollbar: a tick at every glyph
// declaration, and at every problem, placed where the scrollbar's own
// thumb would put that line. Hover names it; click scrolls to it.

import { forEachDiagnostic } from "@codemirror/lint";
import type { EditorState } from "@codemirror/state";
import {
    EditorView,
    type PluginValue,
    ViewPlugin,
    type ViewUpdate,
} from "@codemirror/view";

/** A glyph declaration at the start of a line: `glyph name`. */
const GLYPH = /^glyph[ \t]+([A-Za-z_][A-Za-z0-9_]*)/gm;

export interface NavMark {
    /** Document offset it jumps to. */
    pos: number;
    kind: "glyph" | "error" | "warning";
    /** The hover text. */
    label: string;
    /** What the strip prints beside the tick, room permitting: a glyph's
     * character, or the start of its name when it has no codepoint. */
    short?: string;
}

/** A glyph's first `codepoint` in its header: `'A'`, `U+0041`, `0x41`,
 * or `65`. */
const CODEPOINT =
    /codepoint\s*:\s*\[?\s*(?:'((?:\\.|[^'\\])+)'|U\+([0-9A-Fa-f]+)|0x([0-9A-Fa-f]+)|(\d+))/;
/** A character literal's escapes (spec §5.1). */
const ESCAPES: Record<string, string> = {
    "'": "'",
    "\\": "\\",
    n: "\n",
    t: "\t",
};

function codepointOf(header: string): number | null {
    const m = CODEPOINT.exec(header);
    if (!m) return null;
    if (m[1] !== undefined) {
        const ch = m[1].startsWith("\\") ? (ESCAPES[m[1][1]] ?? m[1][1]) : m[1];
        return ch.codePointAt(0) ?? null;
    }
    const cp = m[2]
        ? Number.parseInt(m[2], 16)
        : m[3]
          ? Number.parseInt(m[3], 16)
          : Number(m[4]);
    return Number.isFinite(cp) && cp <= 0x10ffff ? cp : null;
}

/** How a codepoint reads in the strip: itself, or a stand-in for one
 * that prints nothing. */
function shown(cp: number): string {
    if (cp === 0x20) return "␣";
    const ch = String.fromCodePoint(cp);
    return /^[\p{L}\p{N}\p{P}\p{S}]$/u.test(ch) ? ch : "·";
}

/** The glyph declarations in `text`, in order. */
export function glyphMarks(text: string): NavMark[] {
    return [...text.matchAll(GLYPH)].map((m) => {
        // The header runs to the body's `{`.
        const end = text.indexOf("{", m.index);
        const header = text.slice(m.index, end < 0 ? m.index + 400 : end);
        const cp = codepointOf(header);
        const hex = cp?.toString(16).toUpperCase().padStart(4, "0");
        return {
            pos: m.index,
            kind: "glyph",
            label: cp === null ? `glyph ${m[1]}` : `glyph ${m[1]} · U+${hex}`,
            short: cp === null ? m[1].slice(0, 3) : shown(cp),
        };
    });
}

function problemMarks(state: EditorState): NavMark[] {
    const marks: NavMark[] = [];
    forEachDiagnostic(state, (d, from) => {
        marks.push({
            pos: from,
            kind: d.severity === "error" ? "error" : "warning",
            label: d.message.split("\n")[0],
        });
    });
    return marks;
}

/** Vertical room, in pixels, one label needs. */
const LABEL_GAP = 11;

class NavStrip implements PluginValue {
    readonly view: EditorView;
    readonly dom: HTMLElement;

    constructor(view: EditorView) {
        this.view = view;
        this.dom = document.createElement("div");
        this.dom.className = "cm-mg-nav";
        this.dom.setAttribute("aria-label", "Glyph and problem locations");
        view.dom.appendChild(this.dom);
        this.schedule();
    }

    update(update: ViewUpdate) {
        if (
            update.docChanged ||
            update.geometryChanged ||
            update.transactions.length > 0
        ) {
            this.schedule();
        }
    }

    /** Redraws in CodeMirror's measure phase: placing ticks reads the
     * strip's height, which must not happen mid-update. */
    schedule() {
        this.view.requestMeasure({
            read: (view) => ({
                height: this.dom.clientHeight,
                content: view.contentHeight,
            }),
            write: ({ height, content }, view) =>
                this.draw(view, height, content),
        });
    }

    draw(view: EditorView, height: number, content: number) {
        const { state } = view;
        // Problems first, so glyph ticks (with labels) draw over them.
        const marks = [
            ...problemMarks(state),
            ...glyphMarks(state.doc.toString()),
        ];
        // Labels go top to bottom, each only if it clears the one above.
        let lastLabel = Number.NEGATIVE_INFINITY;
        const ticks = marks.map((mark) => {
            const top = view.lineBlockAt(
                Math.min(mark.pos, state.doc.length),
            ).top;
            const y = Math.round((top / Math.max(content, 1)) * height);
            const tick = document.createElement("button");
            tick.type = "button";
            tick.className = `cm-mg-nav-tick cm-mg-nav-${mark.kind}`;
            tick.style.top = `${y}px`;
            tick.title = mark.label;
            if (mark.short && y - lastLabel >= LABEL_GAP) {
                const label = document.createElement("span");
                label.className = "cm-mg-nav-label";
                label.textContent = mark.short;
                tick.appendChild(label);
                lastLabel = y;
            }
            tick.tabIndex = -1;
            tick.addEventListener("mousedown", (e) => {
                e.preventDefault();
                view.dispatch({
                    selection: { anchor: mark.pos },
                    effects: EditorView.scrollIntoView(mark.pos, {
                        y: "start",
                        yMargin: 24,
                    }),
                });
                view.focus();
            });
            return tick;
        });
        this.dom.replaceChildren(...ticks);
    }

    destroy() {
        this.dom.remove();
    }
}

export const navStrip = ViewPlugin.fromClass(NavStrip);
