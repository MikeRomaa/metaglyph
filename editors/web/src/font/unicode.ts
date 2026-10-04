/** Unicode character names (`HEX;NAME` lines, from
 * `scripts/unicode-names.py`), for finding characters outside the
 * character sets. */
export type Names = Map<number, string>;

export function parseNames(text: string): Names {
    const names: Names = new Map();
    for (const line of text.split("\n")) {
        if (!line || line.startsWith("#")) continue;
        const semi = line.indexOf(";");
        names.set(
            Number.parseInt(line.slice(0, semi), 16),
            line.slice(semi + 1),
        );
    }
    return names;
}

let loading: Promise<Names> | null = null;

/** The name table, loaded once on first use (about 250 KB gzipped). */
export function loadNames(): Promise<Names> {
    loading ??= import("../data/unicode-names.txt?raw").then((m) =>
        parseNames(m.default),
    );
    return loading;
}

/** Whether `cp` can be a glyph's codepoint: in range, not a surrogate. */
function usable(cp: number): boolean {
    return cp >= 0 && cp <= 0x10ffff && !(cp >= 0xd800 && cp <= 0xdfff);
}

/** A codepoint written as `U+00E9`, `0xE9`, or bare hex of 4–6 digits. */
function codepointQuery(query: string): number | null {
    const m = /^(?:u\+|0x)?([0-9a-f]+)$/i.exec(query);
    if (!m) return null;
    const prefixed = m[1].length !== query.length;
    if (!prefixed && (m[1].length < 4 || m[1].length > 6)) return null;
    const cp = Number.parseInt(m[1], 16);
    return usable(cp) ? cp : null;
}

/**
 * The codepoints `query` finds, best first, at most `limit`:
 * - a codepoint (`U+2192`, `0x2192`, `2192`);
 * - pasted characters, each one (any query with a character that can't be
 *   in a name, or a single character);
 * - names in which every word of the query starts a word: `arr right`
 *   finds RIGHTWARDS ARROW. Exact names first, then shorter names.
 */
export function search(query: string, names: Names, limit = 1000): number[] {
    const q = query.trim();
    if (!q) return [];
    const out: number[] = [];
    const seen = new Set<number>();
    const add = (cp: number) => {
        if (!seen.has(cp) && usable(cp)) {
            seen.add(cp);
            out.push(cp);
        }
    };

    const cp = codepointQuery(q);
    if (cp !== null) add(cp);

    // `U+…` and `0x…` are codepoints, never pasted text or names.
    if (/^(?:u\+|0x)[0-9a-f]+$/i.test(q)) return out;

    const chars = [...q];
    if (chars.length === 1 || /[^A-Za-z0-9 -]/.test(q)) {
        for (const ch of chars) {
            if (ch.trim()) add(ch.codePointAt(0) as number);
        }
        // Pasted text, not a name to look up.
        if (chars.length > 1) return out.slice(0, limit);
    }

    const words = q
        .toUpperCase()
        .split(/[\s-]+/)
        .filter(Boolean);
    const exact = q.toUpperCase();
    const hits: [number, string][] = [];
    for (const [c, name] of names) {
        const nameWords = name.split(/[ -]/);
        if (words.every((w) => nameWords.some((n) => n.startsWith(w)))) {
            hits.push([c, name]);
        }
    }
    hits.sort(
        ([a, an], [b, bn]) =>
            Number(bn === exact) - Number(an === exact) ||
            an.length - bn.length ||
            a - b,
    );
    for (const [c] of hits) {
        if (out.length >= limit) break;
        add(c);
    }
    return out;
}
