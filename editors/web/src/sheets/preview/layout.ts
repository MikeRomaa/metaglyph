// Setting preview text (05 PREVIEW): characters to glyphs, glyphs to
// positions with kerning, lines wrapped to a width, and the caret's place
// among them. Pure, so the sheet only draws what this returns.
//
// Offsets into the text are UTF-16 code units, as a textarea's selection
// counts them.

import type { FontData, GlyphInfo } from "../../engine/types.ts";
import { sequenceId, vsNumber } from "../../font/chars.ts";
import { effectiveKern, glyphsByCodepoint } from "../../font/lookup.ts";

/** One character set on a line: its glyph, or none (shown as a box). */
export interface Placed {
    /** The text it sets: one character, or a variation sequence. */
    text: string;
    /** Where that text starts in the whole text. */
    index: number;
    glyph: GlyphInfo | null;
    /** Its origin, in font units from the line's start. */
    x: number;
    /** How far it moves the pen, kerning before it excluded. */
    advance: number;
    /** The kern applied between the previous glyph and this one. */
    kern: number;
}

type Shaped = Omit<Placed, "x" | "kern">;

export interface Line {
    items: Placed[];
    width: number;
    /** The text it sets, `[start, end)`; `end` is the next line's start,
     * or its paragraph's end. */
    start: number;
    end: number;
}

/** A missing character's box takes half an em. */
const MISSING = 0.5;

/** `text`'s characters as glyphs, a variation selector joining the
 * character before it when the font maps that sequence (spec §5.6).
 * `offset` is where `text` starts in the whole text. */
export function shape(font: FontData, text: string, offset = 0): Shaped[] {
    const map = glyphsByCodepoint(font);
    const chars = [...text];
    const out: Shaped[] = [];
    let index = offset;
    for (let i = 0; i < chars.length; i++) {
        const cp = chars[i].codePointAt(0) as number;
        const next = chars[i + 1]?.codePointAt(0);
        if (next !== undefined && vsNumber(next) !== null) {
            const seq = map.get(sequenceId(cp, next));
            if (seq) {
                const text = chars[i] + chars[i + 1];
                out.push({
                    text,
                    index,
                    glyph: seq,
                    advance: seq.advance ?? 0,
                });
                index += text.length;
                i++;
                continue;
            }
        }
        // A selector the font doesn't map falls back to the base alone.
        if (vsNumber(cp) === null) {
            const glyph = map.get(cp) ?? null;
            out.push({
                text: chars[i],
                index,
                glyph,
                advance: glyph ? (glyph.advance ?? 0) : font.em * MISSING,
            });
        }
        index += chars[i].length;
    }
    return out;
}

/** `shaped` laid out from 0, kerned (spec §12.2) when `kern` is on. */
export function place(
    font: FontData,
    shaped: Shaped[],
    kern: boolean,
): { items: Placed[]; width: number } {
    let x = 0;
    const items = shaped.map((item, i) => {
        const before = shaped[i - 1];
        const k =
            kern && before?.glyph && item.glyph
                ? (effectiveKern(font, before.glyph.name, item.glyph.name)
                      ?.value ?? 0)
                : 0;
        x += k;
        const placed = { ...item, x, kern: k };
        x += item.advance;
        return placed;
    });
    return { items, width: x };
}

const isSpace = (item: Shaped) => item.text === " ";

/**
 * `text` as lines no wider than `maxWidth` font units: each paragraph
 * (`\n`) wrapped greedily after runs of spaces, which stay at the end of
 * their line; a word wider than the line gets a line of its own.
 */
export function layout(
    font: FontData,
    text: string,
    maxWidth: number,
    kern: boolean,
): Line[] {
    const lines: Line[] = [];
    let offset = 0;
    for (const paragraph of text.split("\n")) {
        const shaped = shape(font, paragraph, offset);
        const end = offset + paragraph.length;
        if (shaped.length === 0) {
            lines.push({ items: [], width: 0, start: offset, end });
        }
        let from = 0;
        while (from < shaped.length) {
            // The furthest break that fits, or the first one at least.
            let cut = -1;
            for (let k = from; k < shaped.length; k++) {
                const last = k === shaped.length - 1;
                const breaks =
                    isSpace(shaped[k]) && !isSpace(shaped[k + 1] ?? shaped[k]);
                if (!last && !breaks) continue;
                let visible = k + 1;
                while (visible > from && isSpace(shaped[visible - 1]))
                    visible--;
                const fits =
                    place(font, shaped.slice(from, visible), kern).width <=
                    maxWidth;
                if (fits || cut < 0) cut = k + 1;
                if (!fits) break;
            }
            const lineItems = shaped.slice(from, cut);
            lines.push({
                ...place(font, lineItems, kern),
                start: lineItems[0].index,
                end: cut < shaped.length ? shaped[cut].index : end,
            });
            from = cut;
        }
        offset = end + 1;
    }
    return lines;
}

/** The line holding text offset `index`: at a wrap, the line it starts. */
export function lineOf(lines: Line[], index: number): number {
    let found = 0;
    lines.forEach((line, i) => {
        if (line.start <= index) found = i;
    });
    return found;
}

/** Where a caret before text offset `index` sits on line `n`, in font
 * units: before the first glyph at or after it, else at the line's end. */
export function caretX(line: Line, index: number): number {
    const at = line.items.find((item) => item.index >= index);
    return at ? at.x : line.width;
}

/** The text offset nearest `x` (font units) on `line`: a glyph boundary. */
export function indexAt(line: Line, x: number): number {
    let best = line.end;
    let bestDistance = Math.abs(line.width - x);
    for (const item of line.items) {
        const distance = Math.abs(item.x - x);
        if (distance < bestDistance) {
            best = item.index;
            bestDistance = distance;
        }
    }
    return best;
}
