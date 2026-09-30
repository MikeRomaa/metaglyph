import { create } from "zustand";
import type {
    DocState,
    DragInfo,
    DragStep,
    EngineResult,
    FontData,
    GlyphScene,
    LineRef,
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

/** The relationship tools (plan 5, §1.4), as the design abbreviates them. */
export type RelateKind = "CO" | "IX" | "PJ" | "FR" | "PL" | "MR";

/** A relationship tool's pick: a named point, or a line. */
export type RelatePick =
    | { type: "point"; name: string }
    | { type: "line"; line: LineRef };

export type SelectionKind =
    | "point"
    | "line"
    | "path"
    | "segment"
    | "component"
    | "measure"
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
    /** A declaration an edit just created: selected once a result shows
     * it, and offered for rename (plan 5, §1.3). */
    pending: { kind: SelectionKind; name: string; rename: boolean } | null;
    /** `kind:name` of the selection whose name field is open. */
    renaming: string | null;

    // Glyph sheet (canvas settings are not in the source).
    layers: Record<Layer, boolean>;
    tool: Tool;
    /** The path the path tool is drawing, and its first and last points. */
    draft: { path: string; start: Pt; last: Pt } | null;
    /** The path edited last in the active glyph: new paths copy its
     * `stroke`, `caps` and `joins` (plan 5, §1.4). */
    lastPath: string | null;
    /** The glyph the component tool places. */
    componentTarget: string | null;
    /** A point drag in progress (plan 5, §1.5): what it drives, and its
     * latest step. */
    drag: { target: string; info: DragInfo; step: DragStep | null } | null;
    /** A relationship tool waiting for its picks (plan 5, §1.4). */
    relate: { kind: RelateKind; target: string; picks: RelatePick[] } | null;
    /** The selected point's drivers, for the DRIVERS panel. */
    drivers: DragInfo | null;
    // Glyphs sheet.
    charset: number;
    picked: number[];
    /** Glyphs added from the 01 sheet this session, tagged NEW. */
    added: string[];
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
    setPending(kind: SelectionKind, name: string, rename: boolean): void;
    setRenaming(key: string | null): void;
    toggleLayer(layer: Layer): void;
    setTool(tool: Tool): void;
    setDraft(draft: Store["draft"]): void;
    setLastPath(path: string | null): void;
    setComponentTarget(glyph: string | null): void;
    setDrag(drag: Store["drag"]): void;
    setRelate(relate: Store["relate"]): void;
    setDrivers(drivers: DragInfo | null): void;
    setCharset(charset: number): void;
    setPicked(picked: number[]): void;
    markAdded(names: string[]): void;
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
    "measure",
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
    pending: null,
    renaming: null,
    draft: null,
    lastPath: null,
    componentTarget: null,
    drag: null,
    relate: null,
    drivers: null,
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
    added: [],
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
            added: [],
            lastOp: null,
            pending: null,
            renaming: null,
            draft: null,
            lastPath: null,
        })),
    setText: (text, version) => set({ text, version, saved: false }),
    applyResult: ({ doc, view }) =>
        set((s) => {
            const lastGood = doc?.evaluated ? doc : s.lastGood;
            // A just-created declaration becomes the selection once it
            // shows up.
            const created =
                s.pending && view.font
                    ? locate(
                          { ...s.pending, span: [0, 0], origin: "canvas" },
                          view.font,
                          view.scene,
                      )
                    : null;
            return {
                doc: doc ?? s.doc,
                lastGood,
                instance: view.instance,
                font: view.font,
                glyph: view.glyph,
                scene: view.scene,
                pending: created ? null : s.pending,
                renaming:
                    created && s.pending?.rename
                        ? `${created.kind}:${created.name}`
                        : s.renaming,
                // The selection follows its declaration through edits (its
                // span moves), and is dropped when that is gone or in
                // another glyph.
                selection:
                    created ??
                    (s.selection &&
                    view.font &&
                    (view.glyph === s.glyph || !IN_GLYPH.has(s.selection.kind))
                        ? locate(s.selection, view.font, view.scene)
                        : null),
                // A just-created kern becomes the active pair.
                kern:
                    created?.kind === "kern"
                        ? Number(created.name)
                        : s.kern !== null &&
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
        set((s) =>
            s.glyph === glyph
                ? { sheet: sheet ?? s.sheet }
                : {
                      glyph,
                      sheet: sheet ?? s.sheet,
                      selection: null,
                      draft: null,
                      lastPath: null,
                  },
        ),
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
    setPending: (kind, name, rename) =>
        set({ pending: { kind, name, rename } }),
    setRenaming: (renaming) => set({ renaming }),
    toggleLayer: (layer) =>
        set((s) => ({ layers: { ...s.layers, [layer]: !s.layers[layer] } })),
    // Switching tools ends a path in progress.
    setTool: (tool) => set({ tool, draft: null }),
    setDraft: (draft) => set({ draft }),
    setLastPath: (lastPath) => set({ lastPath }),
    setComponentTarget: (componentTarget) => set({ componentTarget }),
    setDrag: (drag) => set({ drag }),
    setRelate: (relate) => set({ relate }),
    setDrivers: (drivers) => set({ drivers }),
    setCharset: (charset) => set({ charset, picked: [] }),
    setPicked: (picked) => set({ picked }),
    markAdded: (names) => set((s) => ({ added: [...s.added, ...names] })),
    setSpacingText: (spacingText) => set({ spacingText }),
    setKernContext: (kernContext) => set({ kernContext }),
    setKern: (kern) => set({ kern }),
}));
