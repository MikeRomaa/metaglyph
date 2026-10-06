import mono500 from "@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-500-normal.woff2?url";
import cond600 from "@fontsource/ibm-plex-sans-condensed/files/ibm-plex-sans-condensed-latin-600-normal.woff2?url";
import cond700 from "@fontsource/ibm-plex-sans-condensed/files/ibm-plex-sans-condensed-latin-700-normal.woff2?url";
import type { FontData, FontInfo } from "../../engine/types.ts";
import { download } from "../../state/files.ts";
import { PAPER, type SpecimenOptions, specimenSheets } from "./specimen.ts";

export type SpecimenFormat = "svg" | "png" | "pdf";

/** PNG resolution, dots per inch. */
const DPI = 200;

const FACES: [string, number, string][] = [
    ["IBM Plex Mono", 500, mono500],
    ["IBM Plex Sans Condensed", 600, cond600],
    ["IBM Plex Sans Condensed", 700, cond700],
];

async function dataUrl(url: string): Promise<string> {
    const blob = await (await fetch(url)).blob();
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
    });
}

/** The label fonts inlined, since an SVG drawn as an image (or opened
 * elsewhere) can't load the app's. */
let fontCss: Promise<string> | null = null;
function labelFonts(): Promise<string> {
    fontCss ??= Promise.all(
        FACES.map(
            async ([family, weight, url]) =>
                `@font-face{font-family:'${family}';font-weight:${weight};src:url(${await dataUrl(url)}) format('woff2')}`,
        ),
    ).then((rules) => rules.join(""));
    return fontCss;
}

function today(): string {
    return new Date().toISOString().slice(0, 10);
}

/** `name-specimen.ext`, numbered when there are several sheets. */
function fileNames(base: string, ext: string, n: number): string[] {
    const stem = `${base.replace(/\.mg$/, "")}-specimen`;
    return Array.from({ length: n }, (_, i) =>
        n === 1 ? `${stem}.${ext}` : `${stem}-${i + 1}.${ext}`,
    );
}

async function toPng(svg: string, width: number, height: number) {
    const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
    try {
        const img = new Image();
        img.src = url;
        await img.decode();
        const canvas = document.createElement("canvas");
        canvas.width = width;
        canvas.height = height;
        canvas.getContext("2d")?.drawImage(img, 0, 0, width, height);
        return await new Promise<Blob>((resolve, reject) =>
            canvas.toBlob(
                (b) => (b ? resolve(b) : reject(new Error("PNG failed"))),
                "image/png",
            ),
        );
    } finally {
        URL.revokeObjectURL(url);
    }
}

/** Prints the sheets from a hidden frame; the browser's print dialog
 * saves them as a PDF, one A3 page each. */
async function printSheets(sheets: string[], title: string) {
    const frame = document.createElement("iframe");
    frame.style.cssText =
        "position:fixed;right:0;bottom:0;width:0;height:0;border:0";
    document.body.append(frame);
    const win = frame.contentWindow;
    const doc = frame.contentDocument;
    if (!win || !doc) {
        frame.remove();
        return;
    }
    doc.open();
    doc.write(
        `<!doctype html><html><head><meta charset="utf-8"><title></title><style>@page{size:${PAPER.width}mm ${PAPER.height}mm;margin:0}html,body{margin:0}svg{display:block}svg+svg{break-before:page}</style></head><body>${sheets.join("")}</body></html>`,
    );
    doc.close();
    doc.title = title;
    await doc.fonts.ready;
    // The PDF's default file name comes from the top document's title.
    const before = document.title;
    document.title = title;
    win.addEventListener("afterprint", () => {
        document.title = before;
        frame.remove();
    });
    win.focus();
    win.print();
}

export async function exportSpecimen(
    format: SpecimenFormat,
    font: FontData,
    info: FontInfo | undefined,
    fileName: string,
    options: Pick<SpecimenOptions, "samples" | "kern">,
) {
    const base: SpecimenOptions = {
        ...options,
        info,
        date: today(),
        fontCss: await labelFonts(),
    };
    if (format === "pdf") {
        const sheets = specimenSheets(font, { ...base, paper: "#ffffff" });
        const stem = fileNames(fileName, "pdf", 1)[0].replace(/\.pdf$/, "");
        await printSheets(sheets, stem);
        return;
    }
    if (format === "svg") {
        const sheets = specimenSheets(font, base);
        const names = fileNames(fileName, "svg", sheets.length);
        for (const [i, svg] of sheets.entries()) {
            download(new Blob([svg], { type: "image/svg+xml" }), names[i]);
        }
        return;
    }
    const width = Math.round((PAPER.width / 25.4) * DPI);
    const height = Math.round((PAPER.height / 25.4) * DPI);
    const sheets = specimenSheets(font, {
        ...base,
        size: { width: `${width}`, height: `${height}` },
    });
    const names = fileNames(fileName, "png", sheets.length);
    for (const [i, svg] of sheets.entries()) {
        download(await toPng(svg, width, height), names[i]);
    }
}
