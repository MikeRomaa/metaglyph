import { create } from "zustand";
import type { DocState } from "../engine/types.ts";

export type Sheet = 1 | 2 | 3 | 4;
export type Theme = "light" | "dark";

export const SHEETS: { n: Sheet; label: string }[] = [
    { n: 1, label: "Glyphs" },
    { n: 2, label: "Glyph" },
    { n: 3, label: "Spacing" },
    { n: 4, label: "Kerning" },
];

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
    /** The newest check result whose version matches `version`, or the last
     * one before it while a check is running. */
    doc: DocState | null;
    /** The last check result that parsed, kept while the text doesn't. */
    lastGood: DocState | null;
    sheet: Sheet;
    instance: string | null;
    theme: Theme;
    cursorLine: number;
    undoDepth: number;

    openDoc(fileName: string, text: string): void;
    setText(text: string, version: number): void;
    setDoc(doc: DocState): void;
    setSaved(saved: boolean): void;
    setSheet(sheet: Sheet): void;
    setInstance(instance: string): void;
    setTheme(theme: Theme): void;
    setCursor(line: number, undoDepth: number): void;
}

export const useStore = create<Store>()((set) => ({
    fileName: "untitled.mg",
    initialText: "",
    epoch: 0,
    text: "",
    version: 0,
    saved: true,
    doc: null,
    lastGood: null,
    sheet: 2,
    instance: null,
    theme: "light",
    cursorLine: 1,
    undoDepth: 0,

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
        })),
    setText: (text, version) => set({ text, version, saved: false }),
    setDoc: (doc) =>
        set((s) => {
            const lastGood = doc.parseOk ? doc : s.lastGood;
            const instances = lastGood?.instances ?? [];
            const instance =
                s.instance && instances.includes(s.instance)
                    ? s.instance
                    : (instances[0] ?? null);
            return { doc, lastGood, instance };
        }),
    setSaved: (saved) => set({ saved }),
    setSheet: (sheet) => set({ sheet }),
    setInstance: (instance) => set({ instance }),
    setTheme: (theme) => set({ theme }),
    setCursor: (cursorLine, undoDepth) => set({ cursorLine, undoDepth }),
}));
