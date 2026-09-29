import a22xMono from "../../../../samples/a22x-mono.mg?raw";
import metaglyphSans from "../../../../samples/metaglyph-sans.mg?raw";
import { useStore } from "./store.ts";

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

export const SAMPLES = [
    { fileName: "a22x-mono.mg", text: a22xMono },
    { fileName: "metaglyph-sans.mg", text: metaglyphSans },
];

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

export function exportDoc() {
    const { fileName, text } = useStore.getState();
    const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = fileName.endsWith(".mg") ? fileName : `${fileName}.mg`;
    a.click();
    URL.revokeObjectURL(url);
}
