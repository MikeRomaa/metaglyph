// Type specimen (05 PREVIEW → export): every glyph with ink in a grid, a
// few settings of text, and a title block, drawn as an engineering sheet
// with zone rulers like the app's own frame. Pure: returns SVG source,
// one document per sheet, in millimetres.

import type { FontData, FontInfo, GlyphInfo } from "../../engine/types.ts";
import { hex, verticalExtent } from "../../font/lookup.ts";
import { type Line, layout } from "./layout.ts";

/** A3 landscape, in millimetres. */
export const PAPER = { width: 420, height: 297 };

/** The light theme's tokens (tokens.css): paper is always light. */
const C = {
    bg: "#f6f4ee",
    ink: "#22303f",
    mid: "#5c6773",
    faint: "#8b95a1",
    rule: "#c9cfd6",
    acc: "#c4572b",
};

const MONO = "'IBM Plex Mono', ui-monospace, monospace";
const COND = "'IBM Plex Sans Condensed', 'IBM Plex Sans', sans-serif";

const ZONES_X = ["8", "7", "6", "5", "4", "3", "2", "1"];
const ZONES_Y = ["A", "B", "C", "D", "E", "F"];

/** Paper edge to ruler, ruler band, border to content. */
const EDGE = 8;
const BAND = 6;
const PAD = 6;
/** The border's rectangle. */
const B = {
    x0: EDGE + BAND,
    y0: EDGE + BAND,
    x1: PAPER.width - EDGE - BAND,
    y1: PAPER.height - EDGE - BAND,
};
/** A section's heading row. */
const HEAD = 6;
/** Title block: width, heading row, data rows. */
const TB = { w: 124, head: 8, row: 6.5, rows: 4 };
const TB_H = TB.head + TB.row * TB.rows;
/** The samples band across the first sheet's foot. */
const BOTTOM = 66;
/** Glyph cells: height per width, and the width range. */
const ASPECT = 1.3;
const MIN_CELL = 13;
const MAX_CELL = 40;
/** Sample sizes, millimetres per em, largest first. */
const SIZES = [16, 10, 7, 5];
const SAMPLE_LABEL = 18;
/** The smallest a sample shrinks to fit, millimetres per em. */
const MIN_SAMPLE = 4;

const CONTENT_W = B.x1 - B.x0 - 2 * PAD;
const GRID_TOP = B.y0 + PAD + HEAD;
/** Grid heights with the samples band, and without (later sheets). */
const GRID_H1 = B.y1 - BOTTOM - PAD - GRID_TOP;
const GRID_H2 = B.y1 - TB_H - PAD - GRID_TOP;

export interface Grid {
    cols: number;
    rows: number;
    /** Cell width; height is `cell * ASPECT`. */
    cell: number;
}

export interface SheetPlan {
    glyphs: GlyphInfo[];
    /** Index of the first glyph among all inked ones. */
    first: number;
    grid: Grid;
    samples: boolean;
}

export interface Sample {
    text: string;
    /** Shrink until each paragraph sets on a single line. */
    oneLine?: boolean;
    /** Extra space above each paragraph, in ems. */
    gapBefore?: number[];
}

export interface SpecimenOptions {
    info?: FontInfo;
    /** Texts to set, largest first; only the first sheet shows them. */
    samples: Sample[];
    kern: boolean;
    /** Shown in the title block. */
    date: string;
    /** `@font-face` rules for the label fonts, so the file stands alone. */
    fontCss?: string;
    /** Background; defaults to the app's paper. */
    paper?: string;
    /** The root's `width`/`height`; defaults to millimetres. */
    size?: { width: string; height: string };
}

/** Glyphs that draw something; spaces and empty glyphs have no cell. */
export function inked(font: FontData): GlyphInfo[] {
    return font.glyphs.filter((g) => g.outline.trim() !== "");
}

/** The largest cells (up to `MAX_CELL`) fitting `n` in `w × h`, or null
 * when they'd be narrower than `MIN_CELL`. */
export function fitGrid(n: number, w: number, h: number): Grid | null {
    for (let cols = Math.max(1, Math.ceil(w / MAX_CELL)); ; cols++) {
        const cell = w / cols;
        if (cell < MIN_CELL) return null;
        const rows = Math.ceil(n / cols);
        if (rows * cell * ASPECT <= h) return { cols, rows, cell };
    }
}

/** The most `MIN_CELL` cells `w × h` holds. */
function denseGrid(w: number, h: number): Grid {
    const cols = Math.floor(w / MIN_CELL);
    const cell = w / cols;
    return { cols, rows: Math.floor(h / (cell * ASPECT)), cell };
}

/** `glyphs` over sheets: one when they fit, else the smallest cells on
 * as many as it takes. Only the first sheet carries samples. */
export function planSheets(glyphs: GlyphInfo[]): SheetPlan[] {
    const one = fitGrid(glyphs.length, CONTENT_W, GRID_H1);
    if (one) return [{ glyphs, first: 0, grid: one, samples: true }];
    const sheets: SheetPlan[] = [];
    let first = 0;
    while (first < glyphs.length) {
        const dense = denseGrid(CONTENT_W, first === 0 ? GRID_H1 : GRID_H2);
        const take = glyphs.slice(first, first + dense.cols * dense.rows);
        sheets.push({
            glyphs: take,
            first,
            grid: { ...dense, rows: Math.ceil(take.length / dense.cols) },
            samples: first === 0,
        });
        first += take.length;
    }
    return sheets;
}

/** Every sheet of `font`'s specimen, as SVG documents. */
export function specimenSheets(
    font: FontData,
    options: SpecimenOptions,
): string[] {
    const glyphs = inked(font);
    const plans = planSheets(glyphs);
    return plans.map((plan, i) =>
        sheet(font, glyphs.length, plan, i + 1, plans.length, options),
    );
}

function esc(s: string): string {
    return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

/** `s` cut to `max` characters, with an ellipsis when cut. */
function clip(s: string, max: number): string {
    return s.length > max ? `${s.slice(0, Math.max(0, max - 1))}…` : s;
}

const n2 = (n: number) => Number(n.toFixed(3));

function text(
    x: number,
    y: number,
    s: string,
    o: {
        size: number;
        family?: string;
        weight?: number;
        fill?: string;
        anchor?: "start" | "middle" | "end";
        spacing?: number;
    },
): string {
    const attrs = [
        `x="${n2(x)}"`,
        `y="${n2(y)}"`,
        `font-family="${o.family ?? MONO}"`,
        `font-size="${o.size}"`,
        `font-weight="${o.weight ?? 500}"`,
        `fill="${o.fill ?? C.ink}"`,
    ];
    if (o.anchor) attrs.push(`text-anchor="${o.anchor}"`);
    if (o.spacing) attrs.push(`letter-spacing="${o.spacing}"`);
    return `<text ${attrs.join(" ")}>${esc(s)}</text>`;
}

function line(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    stroke: string,
    width: number,
): string {
    return `<line x1="${n2(x1)}" y1="${n2(y1)}" x2="${n2(x2)}" y2="${n2(y2)}" stroke="${stroke}" stroke-width="${width}"/>`;
}

function rect(
    x: number,
    y: number,
    w: number,
    h: number,
    stroke: string,
    width: number,
    fill = "none",
): string {
    return `<rect x="${n2(x)}" y="${n2(y)}" width="${n2(w)}" height="${n2(h)}" fill="${fill}" stroke="${stroke}" stroke-width="${width}"/>`;
}

/** A section heading, in the sheets' label style. */
function heading(x: number, y: number, label: string, aside?: string) {
    const out = [
        text(x, y + 3.6, label.toUpperCase(), {
            size: 2.8,
            family: COND,
            weight: 600,
            fill: C.mid,
            spacing: 0.4,
        }),
    ];
    if (aside)
        out.push(
            text(x + label.length * 2.2 + 3, y + 3.6, aside, {
                size: 2.4,
                fill: C.faint,
            }),
        );
    return out.join("");
}

/** The zone rulers and border, as `Frame` draws the app. */
function frame(): string {
    const out: string[] = [];
    out.push(
        rect(
            EDGE,
            EDGE,
            PAPER.width - 2 * EDGE,
            PAPER.height - 2 * EDGE,
            C.ink,
            0.25,
        ),
    );
    const w = (B.x1 - B.x0) / ZONES_X.length;
    const h = (B.y1 - B.y0) / ZONES_Y.length;
    const label = { size: 2.6, fill: C.mid, anchor: "middle" as const };
    ZONES_X.forEach((z, i) => {
        const x = B.x0 + i * w;
        if (i > 0) {
            out.push(line(x, EDGE, x, B.y0, C.rule, 0.25));
            out.push(line(x, B.y1, x, PAPER.height - EDGE, C.rule, 0.25));
        }
        out.push(text(x + w / 2, EDGE + BAND / 2 + 0.9, z, label));
        out.push(text(x + w / 2, B.y1 + BAND / 2 + 0.9, z, label));
    });
    ZONES_Y.forEach((z, i) => {
        const y = B.y0 + i * h;
        if (i > 0) {
            out.push(line(EDGE, y, B.x0, y, C.rule, 0.25));
            out.push(line(B.x1, y, PAPER.width - EDGE, y, C.rule, 0.25));
        }
        out.push(text(EDGE + BAND / 2, y + h / 2 + 0.9, z, label));
        out.push(text(B.x1 + BAND / 2, y + h / 2 + 0.9, z, label));
    });
    // Centring marks, as on a drawing sheet.
    const mx = PAPER.width / 2;
    const my = PAPER.height / 2;
    out.push(line(mx, EDGE, mx, B.y0 + 3, C.ink, 0.35));
    out.push(line(mx, B.y1 - 3, mx, PAPER.height - EDGE, C.ink, 0.35));
    out.push(line(EDGE, my, B.x0 + 3, my, C.ink, 0.35));
    out.push(line(B.x1 - 3, my, PAPER.width - EDGE, my, C.ink, 0.35));
    out.push(rect(B.x0, B.y0, B.x1 - B.x0, B.y1 - B.y0, C.ink, 0.7));
    return out.join("");
}

/** One glyph's cell: name, codepoint, baseline and advance marks. */
function cell(
    x: number,
    y: number,
    w: number,
    h: number,
    glyph: GlyphInfo,
    id: string,
    descender: number,
    ascender: number,
): string {
    const out = [rect(x, y, w, h, C.rule, 0.15)];
    const size = Math.min(2.2, w * 0.13);
    const chars = Math.floor((w - 1.4) / (size * 0.6));
    out.push(
        text(x + 0.8, y + 0.8 + size, clip(glyph.name, chars), {
            size,
            fill: C.mid,
        }),
    );
    const cp = glyph.codepoints[0];
    out.push(
        text(x + 0.8, y + h - 1, cp === undefined ? "—" : `U+${hex(cp)}`, {
            size,
            fill: C.faint,
        }),
    );
    const top = y + size + 2;
    const room = h - 2 * size - 4;
    const span = ascender - descender;
    const advance = glyph.advance ?? glyph.ink?.[2] ?? span * 0.5;
    const s = Math.min(room / span, (w - 2) / Math.max(advance, 1));
    const gx = x + (w - advance * s) / 2;
    const base = top + (room - span * s) / 2 + ascender * s;
    out.push(line(x + 0.5, base, x + w - 0.5, base, C.acc, 0.12));
    for (const at of [gx, gx + advance * s]) {
        out.push(
            line(
                at,
                base - ascender * s,
                at,
                base - descender * s,
                C.rule,
                0.1,
            ),
        );
    }
    out.push(
        `<use xlink:href="#${id}" fill="${C.ink}" transform="translate(${n2(gx)} ${n2(base)}) scale(${n2(s)} ${n2(-s)})"/>`,
    );
    return out.join("");
}

function titleBlock(
    font: FontData,
    page: number,
    pages: number,
    o: SpecimenOptions,
): string {
    const x = B.x1 - TB.w;
    const y = B.y1 - TB_H;
    const info = o.info;
    const out = [rect(x, y, TB.w, TB_H, C.ink, 0.5, o.paper ?? C.bg)];
    const name = clip(`${info?.name ?? "Untitled"} · Type specimen`, 52);
    out.push(
        text(x + 2.5, y + 5.4, name.toUpperCase(), {
            size: 3.2,
            family: COND,
            weight: 700,
            spacing: 0.35,
        }),
    );
    const rows: [string, string][][] = [
        [
            ["DSGN", info?.designer ?? "—"],
            ["FNDRY", info?.foundry ?? "—"],
        ],
        [
            ["EM", String(font.em)],
            ["INST", font.instance],
        ],
        [
            ["REV", info?.version ?? "—"],
            ["DATE", o.date],
        ],
        [
            ["LIC", info?.license ?? "—"],
            ["SHEET", `${page} of ${pages}`],
        ],
    ];
    const col = TB.w / 2;
    const size = 2.3;
    const chars = Math.floor((col - 15) / (size * 0.6));
    rows.forEach((row, r) => {
        const ry = y + TB.head + r * TB.row;
        out.push(line(x, ry, x + TB.w, ry, C.ink, 0.25));
        row.forEach(([key, value], c) => {
            const cx = x + c * col;
            out.push(
                text(cx + 2.5, ry + 4.3, key, { size, fill: C.faint }),
                text(cx + 13, ry + 4.3, clip(value, chars), { size }),
            );
        });
    });
    out.push(line(x + col, y + TB.head, x + col, y + TB_H, C.ink, 0.25));
    return out.join("");
}

/** `d` with its numbers rounded to 0.01 units. Chrome's PDF writer drops
 * a path holding a value like `-6.1e-15`, and the precision is moot. */
export function tidy(d: string): string {
    return d.replace(/-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?/gi, (n) =>
        String(Number(Number(n).toFixed(2)) || 0),
    );
}

/** Glyph ids in `<defs>`, by name, filled as glyphs are drawn. */
class Defs {
    private ids = new Map<string, string>();
    private paths: string[] = [];
    id(glyph: GlyphInfo): string {
        let id = this.ids.get(glyph.name);
        if (!id) {
            id = `g${this.ids.size}`;
            this.ids.set(glyph.name, id);
            this.paths.push(
                `<path id="${id}" d="${esc(tidy(glyph.outline))}"/>`,
            );
        }
        return id;
    }
    toString(): string {
        return this.paths.join("");
    }
}

/** The samples band: full width down to `notch`, where the title block
 * begins and rows narrow to `narrow`. */
interface Band {
    x: number;
    y0: number;
    y1: number;
    wide: number;
    narrow: number;
    notch: number;
}

function samples(
    font: FontData,
    o: SpecimenOptions,
    defs: Defs,
    band: Band,
): string {
    const { x, y0, y1 } = band;
    const [descender, ascender] = verticalExtent(font);
    const out: string[] = [];
    // A taller band (a short grid above) sets everything larger.
    const grow = Math.min(1.6, Math.max(1, (y1 - y0) / (BOTTOM - HEAD)));
    const set = (sample: Sample, top: number, mm: number) => {
        const scale = mm / font.em;
        const height = (ascender - descender) * scale;
        const gapsMm = (sample.gapBefore ?? []).map((g) => g * mm);
        // Gaps count against the notch in full, to be safe.
        const extra = gapsMm.reduce((a, b) => a + b, 0);
        const room = (n: number) =>
            ((top + (n + 1) * height + extra > band.notch
                ? band.narrow
                : band.wide) -
                SAMPLE_LABEL) /
            scale;
        const done = (rows: Line[], ok: boolean) => {
            // A paragraph's first row takes its gap.
            const gaps = rows.map((row) =>
                row.start > 0 && sample.text[row.start - 1] === "\n"
                    ? (gapsMm[
                          sample.text.slice(0, row.start).split("\n").length - 1
                      ] ?? 0)
                    : 0,
            );
            const total =
                rows.length * height + gaps.reduce((a, b) => a + b, 0);
            return { mm, scale, rows, height, gaps, total, ok };
        };
        if (sample.oneLine) {
            const rows = layout(font, sample.text, Infinity, o.kern);
            if (rows.every((l, n) => l.width <= room(n)))
                return done(rows, true);
        }
        const rows = layout(font, sample.text, room, o.kern, true);
        return done(rows, !sample.oneLine);
    };
    let y = y0;
    let drawn = 0;
    o.samples.forEach((sample, k) => {
        // Shrink to the space left, down to a floor; skip if even that won't do.
        let s = set(sample, y, SIZES[Math.min(k, SIZES.length - 1)] * grow);
        while ((!s.ok || y + s.total > y1) && s.mm > MIN_SAMPLE)
            s = set(sample, y, Math.max(MIN_SAMPLE, s.mm * 0.92));
        if (y + s.total > y1) return;
        drawn++;
        out.push(
            text(x, y + 3, `S${drawn}`, {
                size: 2.6,
                family: COND,
                weight: 600,
                fill: C.acc,
                spacing: 0.3,
            }),
            text(x, y + 6, `${Math.round((s.mm * 72) / 25.4)} PT`, {
                size: 2,
                fill: C.faint,
            }),
        );
        for (const [i, row] of s.rows.entries()) {
            y += s.gaps[i];
            const base = y + ascender * s.scale;
            const uses = row.items
                .filter((item) => item.glyph)
                .map(
                    (item) =>
                        `<use xlink:href="#${defs.id(item.glyph as GlyphInfo)}" x="${n2(item.x)}"/>`,
                );
            out.push(
                `<g fill="${C.ink}" transform="translate(${n2(x + SAMPLE_LABEL)} ${n2(base)}) scale(${n2(s.scale)} ${n2(-s.scale)})">${uses.join("")}</g>`,
            );
            y += s.height;
        }
        y += 2.5;
    });
    // Outlines can still reach past their advance; keep them in the band
    // and off the title block.
    const clip = [
        `<rect x="${n2(x)}" y="${n2(y0 - 4)}" width="${n2(band.wide)}" height="${n2(band.notch - y0 + 4)}"/>`,
        `<rect x="${n2(x)}" y="${n2(band.notch)}" width="${n2(band.narrow)}" height="${n2(y1 - band.notch)}"/>`,
    ];
    return `<clipPath id="samples">${clip.join("")}</clipPath><g clip-path="url(#samples)">${out.join("")}</g>`;
}

function sheet(
    font: FontData,
    total: number,
    plan: SheetPlan,
    page: number,
    pages: number,
    o: SpecimenOptions,
): string {
    const [descender, ascender] = verticalExtent(font);
    const defs = new Defs();
    const body: string[] = [frame()];
    const x0 = B.x0 + PAD;

    const range =
        pages > 1
            ? `${plan.first + 1}–${plan.first + plan.glyphs.length} of ${total}`
            : String(total);
    body.push(heading(x0, B.y0 + PAD - 1, "Glyphs", range));
    const { cols, cell: w } = plan.grid;
    const h = w * ASPECT;
    plan.glyphs.forEach((glyph, i) => {
        const cx = x0 + (i % cols) * w;
        const cy = GRID_TOP + Math.floor(i / cols) * h;
        body.push(
            cell(cx, cy, w, h, glyph, defs.id(glyph), descender, ascender),
        );
    });

    if (plan.samples && o.samples.length > 0) {
        // The band reaches up to the grid when the grid is short.
        const gridEnd = GRID_TOP + plan.grid.rows * h;
        const top = Math.min(B.y1 - BOTTOM, gridEnd + PAD);
        body.push(line(B.x0, top, B.x1, top, C.ink, 0.5));
        body.push(heading(x0, top + PAD - 2, "Samples"));
        body.push(
            samples(font, o, defs, {
                x: x0,
                y0: top + PAD + HEAD - 2,
                y1: B.y1 - PAD / 2,
                wide: B.x1 - PAD - x0,
                narrow: B.x1 - TB.w - PAD - x0,
                notch: B.y1 - TB_H - 2,
            }),
        );
    }

    body.push(titleBlock(font, page, pages, o));

    const size = o.size ?? {
        width: `${PAPER.width}mm`,
        height: `${PAPER.height}mm`,
    };
    const style = o.fontCss ? `<style>${o.fontCss}</style>` : "";
    return [
        `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${size.width}" height="${size.height}" viewBox="0 0 ${PAPER.width} ${PAPER.height}">`,
        `<title>${esc(`${o.info?.name ?? "Untitled"} type specimen, sheet ${page} of ${pages}`)}</title>`,
        `<defs>${style}${defs}</defs>`,
        `<rect width="${PAPER.width}" height="${PAPER.height}" fill="${o.paper ?? C.bg}"/>`,
        body.join(""),
        "</svg>",
    ].join("");
}
