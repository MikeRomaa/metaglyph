// What a glyph can be mapped from: a codepoint, or a variation sequence
// (spec §5.6) — a base character and a variation selector. The 01 sheet
// keys cells and picks by one number for either: a codepoint is itself,
// and a sequence is packed above the codepoint range as
// `0x110000 · n + base`, where `n` is the selector's number (VS1–VS256).

const PLANE = 0x110000;

/** A codepoint, or a packed variation sequence. */
export type CharId = number;

/** The number of variation selector `cp` (VS1–VS256), or null. */
export function vsNumber(cp: number): number | null {
    if (cp >= 0xfe00 && cp <= 0xfe0f) return cp - 0xfe00 + 1;
    if (cp >= 0xe0100 && cp <= 0xe01ef) return cp - 0xe0100 + 17;
    return null;
}

function selectorOf(n: number): number {
    return n <= 16 ? 0xfe00 + n - 1 : 0xe0100 + n - 17;
}

/** The id of `base` followed by `selector`; `selector` must be one. */
export function sequenceId(base: number, selector: number): CharId {
    const n = vsNumber(selector);
    if (n === null) throw new Error(`U+${selector.toString(16)} is not a VS`);
    return PLANE * n + base;
}

export function unpack(id: CharId): { base: number; selector?: number } {
    if (id < PLANE) return { base: id };
    return {
        base: id % PLANE,
        selector: selectorOf(Math.floor(id / PLANE)),
    };
}

export function hex(cp: number): string {
    return cp.toString(16).toUpperCase().padStart(4, "0");
}

/** `0030`, or `0030 FE00` for a sequence. */
export function charLabel(id: CharId): string {
    const { base, selector } = unpack(id);
    return selector === undefined ? hex(base) : `${hex(base)} ${hex(selector)}`;
}

/** `0030`, or `0030 VS1` for a sequence: a grid cell's label. */
export function shortLabel(id: CharId): string {
    const { base, selector } = unpack(id);
    return selector === undefined
        ? hex(base)
        : `${hex(base)} VS${vsNumber(selector)}`;
}

/** `U+0030`, or `U+0030 U+FE00`. */
export function charCode(id: CharId): string {
    const { base, selector } = unpack(id);
    return selector === undefined
        ? `U+${hex(base)}`
        : `U+${hex(base)} U+${hex(selector)}`;
}
