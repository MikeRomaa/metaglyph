import { type CharId, hex, sequenceId, unpack, vsNumber } from "./chars.ts";

/** Unicode character names and variation-sequence descriptions (from
 * `scripts/unicode-data.py`), for finding characters outside the
 * character sets. */
export interface Names {
    /** Codepoint → name. */
    chars: Map<number, string>;
    /** Packed sequence (see `chars.ts`) → description, such as "short
     * diagonal stroke form". */
    sequences: Map<CharId, string>;
}

function lines(text: string): [string, string][] {
    return text
        .split("\n")
        .filter((line) => line && !line.startsWith("#"))
        .map((line) => {
            const semi = line.indexOf(";");
            return [line.slice(0, semi), line.slice(semi + 1)];
        });
}

/** Parses `HEX;NAME` and `BASE SELECTOR;DESCRIPTION` lines. */
export function parseNames(names: string, sequences: string): Names {
    const chars = new Map<number, string>();
    for (const [cp, name] of lines(names)) {
        chars.set(Number.parseInt(cp, 16), name);
    }
    const seqs = new Map<CharId, string>();
    for (const [pair, description] of lines(sequences)) {
        const [base, selector] = pair
            .split(" ")
            .map((h) => Number.parseInt(h, 16));
        seqs.set(sequenceId(base, selector), description);
    }
    return { chars, sequences: seqs };
}

let loading: Promise<Names> | null = null;

/** The name tables, loaded once on first use (about 260 KB gzipped). */
export function loadNames(): Promise<Names> {
    loading ??= Promise.all([
        import("../data/unicode-names.txt?raw"),
        import("../data/variation-sequences.txt?raw"),
    ]).then(([names, sequences]) =>
        parseNames(names.default, sequences.default),
    );
    return loading;
}

/** How `id` reads in a title: a character's name, or a sequence's base
 * name and description (`DIGIT ZERO · short diagonal stroke form`). */
export function describe(id: CharId, names: Names): string | undefined {
    const { base, selector } = unpack(id);
    const name = names.chars.get(base);
    if (selector === undefined) return name;
    const what =
        names.sequences.get(id) ??
        `VS${vsNumber(selector)} (U+${hex(selector)})`;
    return `${name ?? `U+${hex(base)}`} · ${what}`;
}

/** Whether `cp` can be a glyph's codepoint: in range, not a surrogate. */
function usable(cp: number): boolean {
    return cp >= 0 && cp <= 0x10ffff && !(cp >= 0xd800 && cp <= 0xdfff);
}

/** A codepoint written as `U+00E9`, `0xE9`, or bare hex of 4–6 digits;
 * `prefixed` when it says so with `U+` or `0x`. */
function codepointToken(
    token: string,
): { cp: number; prefixed: boolean } | null {
    const m = /^(?:u\+|0x)?([0-9a-f]+)$/i.exec(token);
    if (!m) return null;
    const prefixed = m[1].length !== token.length;
    if (!prefixed && (m[1].length < 4 || m[1].length > 6)) return null;
    return { cp: Number.parseInt(m[1], 16), prefixed };
}

/**
 * What `query` finds, best first, at most `limit`:
 * - a codepoint (`U+2192`, `0x2192`, `2192`), or a variation sequence as
 *   two (`U+0030 U+FE00`, `0030 FE00`);
 * - pasted characters, each one, with a variation selector joining the
 *   character before it (any query with a character that can't be in a
 *   name, or a single character);
 * - names in which every word of the query starts a word: `arr right`
 *   finds RIGHTWARDS ARROW, and `zero short diagonal` finds DIGIT ZERO's
 *   short diagonal stroke form. Exact names first, then shorter names.
 */
export function search(query: string, names: Names, limit = 1000): CharId[] {
    const q = query.trim();
    if (!q) return [];
    const out: CharId[] = [];
    const seen = new Set<CharId>();
    /** Adds `cp`, or the sequence `cp` + `selector`, if `cp` is usable. */
    const add = (cp: number, selector?: number) => {
        if (!usable(cp)) return;
        const id = selector === undefined ? cp : sequenceId(cp, selector);
        if (!seen.has(id)) {
            seen.add(id);
            out.push(id);
        }
    };

    const tokens = q.split(/[\s,]+/).map(codepointToken);
    if (tokens.length <= 2 && tokens.every((t) => t !== null)) {
        const [first, second] = tokens as { cp: number; prefixed: boolean }[];
        if (!second) add(first.cp);
        else if (vsNumber(second.cp) !== null) {
            add(first.cp, second.cp);
            add(first.cp);
        }
        // `U+…` and `0x…` are codepoints, never pasted text or names.
        if (tokens.some((t) => t?.prefixed)) return out;
    }

    const chars = [...q].map((ch) => ch.codePointAt(0) as number);
    if (chars.length === 1 || /[^A-Za-z0-9 -]/.test(q)) {
        for (let i = 0; i < chars.length; i++) {
            const next = chars[i + 1];
            if (next !== undefined && vsNumber(next) !== null) {
                add(chars[i], next);
                i++;
            } else if (String.fromCodePoint(chars[i]).trim()) {
                add(chars[i]);
            }
        }
        // Pasted text, not a name to look up.
        if (chars.length > 1) return out.slice(0, limit);
    }

    const words = q
        .toUpperCase()
        .split(/[\s-]+/)
        .filter(Boolean);
    const exact = q.toUpperCase();
    const index = searchIndex(names);
    const hits = matching(index, words);
    hits.sort(
        (a, b) =>
            Number(index.texts[b] === exact) -
                Number(index.texts[a] === exact) ||
            index.texts[a].length - index.texts[b].length ||
            index.ids[a] - index.ids[b],
    );
    for (const entry of hits) {
        if (out.length >= limit) break;
        const { base, selector } = unpack(index.ids[entry]);
        add(base, selector);
    }
    return out;
}

/**
 * Every name and sequence description, split into words once: a sorted
 * vocabulary with, per word, the entries that contain it. A query word
 * then costs a binary search for the first vocabulary word it prefixes
 * and a walk over the run of words it prefixes — what a trie would do,
 * in flat arrays. Built on first search and kept on the `Names`.
 */
interface SearchIndex {
    /** Entry → what it finds: a codepoint or a packed sequence. */
    ids: CharId[];
    /** Entry → its searchable text, upper case: a name, or a sequence's
     * base name and description. */
    texts: string[];
    /** Every word, sorted. */
    vocab: string[];
    /** `vocab[k]` → the entries containing it, ascending. */
    postings: Uint32Array[];
}

const indexes = new WeakMap<Names, SearchIndex>();

function searchIndex(names: Names): SearchIndex {
    const cached = indexes.get(names);
    if (cached) return cached;
    const ids: CharId[] = [];
    const texts: string[] = [];
    for (const [cp, name] of names.chars) {
        ids.push(cp);
        texts.push(name);
    }
    for (const [id, description] of names.sequences) {
        const base = names.chars.get(unpack(id).base) ?? "";
        ids.push(id);
        texts.push(`${base} ${description.toUpperCase()}`);
    }
    const byWord = new Map<string, number[]>();
    texts.forEach((text, entry) => {
        for (const word of new Set(text.split(/[ -]/))) {
            if (!word) continue;
            const list = byWord.get(word);
            if (list) list.push(entry);
            else byWord.set(word, [entry]);
        }
    });
    const vocab = [...byWord.keys()].sort();
    const postings = vocab.map((word) =>
        Uint32Array.from(byWord.get(word) ?? []),
    );
    const index = { ids, texts, vocab, postings };
    indexes.set(names, index);
    return index;
}

/** The entries in which every one of `words` starts some word. */
function matching(index: SearchIndex, words: string[]): number[] {
    // `seen[e]` counts the query words entry `e` has matched so far; an
    // entry only advances on query word `q` if it matched all before it.
    const seen = new Uint8Array(index.ids.length);
    for (let q = 0; q < words.length; q++) {
        const word = words[q];
        for (
            let k = lowerBound(index.vocab, word);
            k < index.vocab.length;
            k++
        ) {
            if (!index.vocab[k].startsWith(word)) break;
            for (const entry of index.postings[k]) {
                if (seen[entry] === q) seen[entry] = q + 1;
            }
        }
    }
    const hits: number[] = [];
    for (let entry = 0; entry < seen.length; entry++) {
        if (seen[entry] === words.length) hits.push(entry);
    }
    return hits;
}

/** The first index in sorted `list` whose word is not before `word`. */
function lowerBound(list: string[], word: string): number {
    let lo = 0;
    let hi = list.length;
    while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (list[mid] < word) lo = mid + 1;
        else hi = mid;
    }
    return lo;
}
