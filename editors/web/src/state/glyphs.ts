// Adding glyphs from the 01 GLYPHS sheet (plan 5, §1.6 "New glyph"):
// each picked codepoint gets its AGLFN name, or `uniXXXX`, editable before
// the insert. The engine checks everything again; this checks as the user
// types.

import { AGLFN } from "../data/aglfn.ts";
import type { FontData } from "../engine/types.ts";
import { type CharId, hex, unpack, vsNumber } from "../font/chars.ts";
import { performEdit } from "./actions.ts";

/** The language's reserved words (spec §5.4). */
const RESERVED = new Set([
    "true",
    "false",
    "and",
    "or",
    "not",
    "font",
    "param",
    "metric",
    "let",
    "glyph",
    "instance",
    "group",
    "kern",
    "path",
    "anchor",
    "component",
    "start",
    "line",
    "quad",
    "cube",
    "arc",
    "close",
]);

/** The default name for `id`: its AGLFN name, else `uniXXXX` (`uXXXXX`
 * past the BMP), as the AGL specification forms them. A variation
 * sequence is its base's name plus `_vsN` (plan 5: `zero_vs1`). */
export function glyphName(id: CharId): string {
    const { base, selector } = unpack(id);
    const name =
        AGLFN.get(base) ??
        (base > 0xffff ? `u${hex(base)}` : `uni${hex(base)}`);
    return selector === undefined ? name : `${name}_vs${vsNumber(selector)}`;
}

export interface NewGlyph {
    codepoint: number;
    /** With a selector, the glyph is for the sequence `codepoint` +
     * `selector` (spec §5.6). */
    selector?: number;
    name: string;
}

/** A picked `id` as a new glyph with its default name. */
export function newGlyph(id: CharId): NewGlyph {
    const { base, selector } = unpack(id);
    return { codepoint: base, selector, name: glyphName(id) };
}

/** Why each name can't be used, by row (`null` when it can): not an
 * identifier, reserved, already declared, or repeated in the list. */
export function nameErrors(
    rows: NewGlyph[],
    font: FontData | null,
): (string | null)[] {
    const taken = new Set([
        ...(font?.glyphs.map((g) => g.name) ?? []),
        ...(font?.groups.map((g) => g.name) ?? []),
        ...(font?.lets.map((l) => l.name) ?? []),
        ...(font?.metrics.map((m) => m.name) ?? []),
    ]);
    const counts = new Map<string, number>();
    for (const row of rows) {
        counts.set(row.name, (counts.get(row.name) ?? 0) + 1);
    }
    return rows.map(({ name }) => {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
            return "not an identifier";
        }
        if (RESERVED.has(name)) return "a reserved word";
        if (taken.has(name)) return "already declared";
        if ((counts.get(name) ?? 0) > 1) return "listed twice";
        return null;
    });
}

/**
 * The `advance` a new, empty glyph declares (mirrors the engine): the
 * font's most common `advance` declaration, first on a tie, else half an
 * em. Never `rsb`/`lsb` or a `glyph.*` advance: those need ink, which an
 * empty glyph lacks.
 */
export function defaultAdvance(font: FontData | null): string {
    const counts = new Map<string, number>();
    for (const glyph of font?.glyphs ?? []) {
        const text = glyph.fields.advance;
        if (text && !text.includes("glyph.")) {
            counts.set(text, (counts.get(text) ?? 0) + 1);
        }
    }
    let best: [string, number] | null = null;
    for (const [text, n] of counts) {
        if (!best || n > best[1]) best = [text, n];
    }
    return best?.[0] ?? String(Math.round((font?.em ?? 1000) / 2));
}

/** How a new glyph's declaration reads (mirrors the engine's text). */
export function glyphDecl(
    { codepoint, selector, name }: NewGlyph,
    advance: string,
): string {
    const c = String.fromCodePoint(codepoint);
    const literal =
        /^[\p{L}\p{N}\p{P}\p{S}]$/u.test(c) && !"'\\".includes(c)
            ? `'${c}'`
            : `U+${hex(codepoint)}`;
    const maps =
        selector === undefined
            ? `codepoint: ${literal}`
            : `variation: (${literal}, U+${hex(selector)})`;
    return `glyph ${name} (${maps}, advance: ${advance}) {\n}`;
}

/** Inserts `rows` as new glyphs: one edit, one undo step. */
export function addGlyphs(rows: NewGlyph[]) {
    return performEdit(
        "add_glyphs",
        () => ({ op: "addGlyphs", glyphs: rows }),
        { rename: false },
    );
}
