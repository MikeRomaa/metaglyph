// Picking missing characters on the 01 grid by dragging across it, like
// selecting text: everything between the cell the drag started on and the
// one under the pointer, in reading order.

/** A cell as picking sees it: its codepoint, if it has one, and whether
 * the font already has a glyph for it (those can't be picked). */
export interface PickCell {
    cp?: number;
    inFont: boolean;
}

/**
 * The cells the coverage strip shows, `[start, end)`: all of them when
 * `capacity` bars fit, else a window of `capacity` centred on `focus`
 * (the middle of what the grid shows), kept inside the set.
 */
export function coverageWindow(
    total: number,
    capacity: number,
    focus: number,
): [number, number] {
    if (total <= capacity) return [0, total];
    const size = Math.max(1, capacity);
    const start = Math.min(
        Math.max(0, Math.round(focus - size / 2)),
        total - size,
    );
    return [start, start + size];
}

/**
 * The picks after dragging from cell `anchor` to cell `current`, starting
 * from the picks `base` the drag began with. `add` picks the range's
 * missing codepoints; otherwise it unpicks them (a drag that starts on a
 * picked cell).
 */
export function dragPick(
    cells: PickCell[],
    anchor: number,
    current: number,
    add: boolean,
    base: number[],
): number[] {
    const [lo, hi] = anchor <= current ? [anchor, current] : [current, anchor];
    const range = new Set<number>();
    for (const cell of cells.slice(lo, hi + 1)) {
        if (!cell.inFont && cell.cp !== undefined) range.add(cell.cp);
    }
    if (add) return [...new Set([...base, ...range])];
    return base.filter((cp) => !range.has(cp));
}
