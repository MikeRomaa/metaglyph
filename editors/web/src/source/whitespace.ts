// Turning formatted text back into small edits (see `format.ts`).

const SPACE = /\s/;

/**
 * The edits that turn `before` into `after` when the two differ only in
 * whitespace — all the formatter ever changes — one per whitespace run
 * that differs, so the cursor and selection map through untouched text.
 * Null when the non-whitespace characters don't line up.
 */
export function whitespaceChanges(
    before: string,
    after: string,
): { from: number; to: number; insert: string }[] | null {
    const changes: { from: number; to: number; insert: string }[] = [];
    let i = 0;
    let j = 0;
    for (;;) {
        const i0 = i;
        const j0 = j;
        while (i < before.length && SPACE.test(before[i])) i++;
        while (j < after.length && SPACE.test(after[j])) j++;
        if (before.slice(i0, i) !== after.slice(j0, j)) {
            changes.push({ from: i0, to: i, insert: after.slice(j0, j) });
        }
        if (i === before.length || j === after.length) {
            return i === before.length && j === after.length ? changes : null;
        }
        if (before[i] !== after[j]) return null;
        i++;
        j++;
    }
}
