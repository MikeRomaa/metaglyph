import { create } from "zustand";
import type {
    DocState,
    EngineResult,
    FontData,
    GlyphScene,
    Pt,
    Span,
} from "../engine/types.ts";
import { locate } from "../font/lookup.ts";

export type Sheet = 1 | 2 | 3 | 4;
export type Theme = "light" | "dark";

export const SHEETS: { n: Sheet; label: string }[] = [
    { n: 1, label: "Glyphs" },
    { n: 2, label: "Glyph" },
    { n: 3, label: "Spacing" },
    { n: 4, label: "Kerning" },
];

export type Layer =
    | "metrics"
    | "guides"
    | "construction"
    | "dims"
    | "skeleton"
    | "outline";

export const LAYERS: { key: Layer; label: string }[] = [
    { key: "metrics", label: "Metrics" },
    { key: "guides", label: "Guides" },
    { key: "construction", label: "Constr" },
    { key: "dims", label: "Dims" },
    { key: "skeleton", label: "Skel" },
    { key: "outline", label: "Outline" },
];

export type Tool = "V" | "P" | "." | "L" | "G" | "M" | "C";

export type SelectionKind =
    | "point"
    | "line"
    | "path"
    | "segment"
    | "component"
    | "glyph"
    | "kern"
    | "metric"
    | "let"
    | "group";

/** What is selected, identified by its declaration's source range. */
export interface Selection {
    kind: SelectionKind;
    /** A name for display; unique only together with `kind`. */
    name: string;
    span: Span;
    /** Where the selection came from: the canvas scrolls the source to it,
     * the source doesn't. */
    origin: "canvas" | "source";
}

interface Store {
    /** The file name shown in the header and used on export. */
    fileName: string;
    /** Text to load into the editor; bumping `epoch` makes it load. */
    initialText: string;
    epoch: number;
    /** Mirrors the editor's text; `version` increments on every change. */
    text: string;
    version: number;
    saved: boolean;
    /** The newest check result; its spans match the text only when its
     * version equals `version`. */
    doc: DocState | null;
    /** The last check result that parsed, kept while the text doesn't. */
    lastGood: DocState | null;
    /** The active instance's data and glyph drawing, from the last good
     * text. */
    font: FontData | null;
    scene: GlyphScene | null;
    sheet: Sheet;
    instance: string | null;
    glyph: string | null;
    selection: Selection | null;
    theme: Theme;
    cursorLine: number;
    undoDepth: number;
    /** The pointer over a drawing, in font units. */
    pointer: Pt | null;
    /** A short message in the status bar: why an edit didn't happen. */
    notice: { text: string; tone: "error" | "info" } | null;
    /** The label of the last canvas edit (`rename`, `delete`, …). */
    lastOp: string | null;

    // Glyph sheet (canvas settings are not in the source).
    layers: Record<Layer, boolean>;
    tool: Tool;
    // Glyphs sheet.
    charset: number;
    picked: number[];
    // Spacing sheet.
    spacingText: string;
    // Kerning sheet.
    kernContext: string;
    kern: number | null;

    openDoc(fileName: string, text: string): void;
    setText(text: string, version: number): void;
    applyResult(result: EngineResult): void;
    setSaved(saved: boolean): void;
    setSheet(sheet: Sheet): void;
    setInstance(instance: string): void;
    setGlyph(glyph: string, sheet?: Sheet): void;
    select(selection: Selection | null): void;
    setTheme(theme: Theme): void;
    setCursor(line: number, undoDepth: number): void;
    setPointer(pointer: Pt | null): void;
    setNotice(text: string, tone?: "error" | "info"): void;
    setLastOp(op: string | null): void;
    toggleLayer(layer: Layer): void;
    setTool(tool: Tool): void;
    setCharset(charset: number): void;
    setPicked(picked: number[]): void;
    setSpacingText(text: string): void;
    setKernContext(text: string): void;
    setKern(kern: number | null): void;
}

let noticeTimer: ReturnType<typeof setTimeout> | undefined;

/** Selections that live inside the active glyph's scene. */
const IN_GLYPH = new Set<SelectionKind>([
    "point",
    "line",
    "path",
    "segment",
    "component",
]);

export const useStore = create<Store>()((set) => ({
    fileName: "untitled.mg",
    initialText: "",
    epoch: 0,
    text: "",
    version: 0,
    saved: true,
    doc: null,
    lastGood: null,
    font: null,
    scene: null,
    sheet: 2,
    instance: null,
    glyph: null,
    selection: null,
    theme: "light",
    cursorLine: 1,
    undoDepth: 0,
    pointer: null,
    notice: null,
    lastOp: null,
    layers: {
        metrics: true,
        guides: true,
        construction: true,
        dims: true,
        skeleton: true,
        outline: true,
    },
    tool: "V",
    charset: 0,
    picked: [],
    spacingText: "",
    kernContext: "nn<pair>nn · HH<pair>HH",
    kern: null,

    openDoc: (fileName, text) =>
        set((s) => ({
            fileName,
            initialText: text,
            epoch: s.epoch + 1,
            text,
            version: s.version + 1,
            saved: false,
            doc: null,
            lastGood: null,
            font: null,
            scene: null,
            glyph: null,
            selection: null,
            spacingText: "",
            kern: null,
            picked: [],
            lastOp: null,
        })),
    setText: (text, version) => set({ text, version, saved: false }),
    applyResult: ({ doc, view }) =>
        set((s) => {
            const lastGood = doc?.parseOk ? doc : s.lastGood;
            return {
                doc: doc ?? s.doc,
                lastGood,
                instance: view.instance,
                font: view.font,
                glyph: view.glyph,
                scene: view.scene,
                // The selection follows its declaration through edits (its
                // span moves), and is dropped when that is gone or in
                // another glyph.
                selection:
                    s.selection &&
                    view.font &&
                    (view.glyph === s.glyph || !IN_GLYPH.has(s.selection.kind))
                        ? locate(s.selection, view.font, view.scene)
                        : null,
                kern:
                    s.kern !== null &&
                    view.font &&
                    s.kern < view.font.kerns.length
                        ? s.kern
                        : null,
            };
        }),
    setSaved: (saved) => set({ saved }),
    setSheet: (sheet) => set({ sheet }),
    setInstance: (instance) => set({ instance }),
    setGlyph: (glyph, sheet) =>
        set((s) => ({
            glyph,
            sheet: sheet ?? s.sheet,
            selection: s.glyph === glyph ? s.selection : null,
        })),
    select: (selection) => set({ selection }),
    setTheme: (theme) => set({ theme }),
    setCursor: (cursorLine, undoDepth) => set({ cursorLine, undoDepth }),
    setPointer: (pointer) => set({ pointer }),
    setNotice: (text, tone = "error") => {
        set({ notice: { text, tone } });
        clearTimeout(noticeTimer);
        noticeTimer = setTimeout(() => set({ notice: null }), 4000);
    },
    setLastOp: (lastOp) => set({ lastOp }),
    toggleLayer: (layer) =>
        set((s) => ({ layers: { ...s.layers, [layer]: !s.layers[layer] } })),
    setTool: (tool) => set({ tool }),
    setCharset: (charset) => set({ charset, picked: [] }),
    setPicked: (picked) => set({ picked }),
    setSpacingText: (spacingText) => set({ spacingText }),
    setKernContext: (kernContext) => set({ kernContext }),
    setKern: (kern) => set({ kern }),
}));
