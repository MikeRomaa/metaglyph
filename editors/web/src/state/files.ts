import metaglyphSans from "../../../../samples/metaglyph-sans.mg?raw";
import { buildTtf } from "../engine/client.ts";
import type { DocState } from "../engine/types.ts";
import { settled } from "./actions.ts";
import { useStore } from "./store.ts";
import { zip } from "./zip.ts";

/** A new project: the font directive, the five required metrics (spec
 * §5.6) and one instance (plan 5, §2.1). */
export const SKELETON = `font (name: "Untitled", em: 1000)

metric baseline  (y: 0)
metric xHeight   (y: 500)
metric capHeight (y: 700)
metric ascender  (y: 800)
metric descender (y: -200)

instance Regular ()
`;

export const SAMPLES = [{ fileName: "metaglyph-sans.mg", text: metaglyphSans }];

export function newDoc() {
    useStore.getState().openDoc("untitled.mg", SKELETON);
}

export async function importFile(file: File) {
    useStore.getState().openDoc(file.name, await file.text());
}

/** Opens a file picker and imports the chosen `.mg` file. */
export function pickFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".mg,text/plain";
    input.onchange = () => {
        const file = input.files?.[0];
        if (file) void importFile(file);
    };
    input.click();
}

/** Saves `blob` as `name` through a download. */
export function download(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
}

export function exportDoc() {
    const { fileName, text } = useStore.getState();
    download(
        new Blob([text], { type: "text/plain" }),
        fileName.endsWith(".mg") ? fileName : `${fileName}.mg`,
    );
}

/** Why TTF export is unavailable for `doc`, or null when it can run: a
 * build needs text that evaluates (spec §4.6). */
export function exportBlocked(doc: DocState | null): string | null {
    if (!doc) return "Checking the source…";
    const errors = doc.diagnostics.filter((d) => d.severity === "error");
    if (!doc.parseOk) return "Fix the syntax errors to export.";
    if (errors.length > 0 || !doc.evaluated) {
        const n = errors.length;
        return `Fix ${n} error${n === 1 ? "" : "s"} in the source to export.`;
    }
    return null;
}

/**
 * Builds every instance and downloads the result: one `.ttf`, or a zip
 * of them for several instances (plan 6, §4 "Shell"). A build error is
 * reported in the status bar; the diagnostics pane has the details.
 */
export async function exportTtf() {
    const store = useStore.getState();
    await settled();
    const blocked = exportBlocked(useStore.getState().doc);
    if (blocked) {
        store.setNotice(blocked);
        return;
    }
    const result = await buildTtf(Math.floor(Date.now() / 1000));
    if (!result) {
        store.setNotice("Fix the errors in the source to export.");
        return;
    }
    const errors = result.diagnostics.filter((d) => d.severity === "error");
    if (result.fonts.length === 0 || errors.length > 0) {
        const first = errors[0]?.message.split("\n")[0] ?? "the build failed";
        store.setNotice(
            `Export failed: ${first}${errors.length > 1 ? ` (+${errors.length - 1} more)` : ""}`,
        );
        return;
    }
    const type = "font/ttf";
    if (result.fonts.length === 1) {
        const font = result.fonts[0];
        download(new Blob([font.data], { type }), font.fileName);
    } else {
        const archive = zip(
            result.fonts.map((f) => ({ name: f.fileName, data: f.data })),
        );
        const base = store.fileName.replace(/\.mg$/, "") || "fonts";
        download(
            new Blob([archive], { type: "application/zip" }),
            `${base}.zip`,
        );
    }
    store.setNotice(
        `Exported ${result.fonts.map((f) => f.fileName).join(", ")}`,
        "info",
    );
}
