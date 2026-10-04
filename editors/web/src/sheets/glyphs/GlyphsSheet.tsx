import { useEffect, useRef, useState } from "react";
import type { FontData, GlyphInfo } from "../../engine/types.ts";
import { CHARSETS } from "../../font/blocks.ts";
import { charCode, charLabel, shortLabel } from "../../font/chars.ts";
import {
    glyphsByCodepoint,
    sampleChar,
    verticalExtent,
} from "../../font/lookup.ts";
import { describe, loadNames, type Names, search } from "../../font/unicode.ts";
import { useStore } from "../../state/store.ts";
import { GlyphThumb } from "../../ui/GlyphThumb.tsx";
import { Centre, Empty, LeftColumn, Section, sheet } from "../../ui/Sheet.tsx";
import { AddGlyphsModal } from "./AddGlyphsModal.tsx";
import styles from "./GlyphsSheet.module.css";
import { coverageWindow, dragPick } from "./pick.ts";

/** One cell: a glyph in the font, or a codepoint it lacks. */
interface Cell {
    key: string;
    cp?: number;
    glyph?: GlyphInfo;
}

function cellsFor(font: FontData, charset: number, found: number[]): Cell[] {
    if (charset === 0) {
        return font.glyphs.map((glyph) => ({
            key: glyph.name,
            cp: glyph.codepoints[0],
            glyph,
        }));
    }
    const map = glyphsByCodepoint(font);
    const cps = charset === -1 ? found : CHARSETS[charset - 1].codepoints;
    return cps.map((cp) => ({
        key: charLabel(cp),
        cp,
        glyph: map.get(cp),
    }));
}

export function GlyphsSheet() {
    const font = useStore((s) => s.font);
    const charset = useStore((s) => s.charset);
    const found = useStore((s) => s.found);
    const query = useStore((s) => s.search);
    const names = useNames(charset === -1);
    const picked = useStore((s) => s.picked);
    const setPicked = useStore((s) => s.setPicked);
    const setGlyph = useStore((s) => s.setGlyph);
    const added = useStore((s) => s.added);
    const markAdded = useStore((s) => s.markAdded);
    const [adding, setAdding] = useState(false);
    const gridRef = useRef<HTMLDivElement>(null);
    /** The cells the grid shows, `[first, last]`, for the coverage strip. */
    const [onScreen, setOnScreen] = useState<[number, number]>([0, 0]);
    /** A drag across the grid picking cells (see `dragPick`). */
    const dragRef = useRef<{
        anchor: number;
        add: boolean;
        base: number[];
        moved: boolean;
    } | null>(null);
    useEffect(() => {
        // The drag ends wherever the pointer is released. Cleared after
        // the click that may follow, so that click can tell it ended a
        // drag and not toggle its cell.
        const end = () =>
            setTimeout(() => {
                dragRef.current = null;
            });
        window.addEventListener("pointerup", end);
        return () => window.removeEventListener("pointerup", end);
    }, []);

    // The grid's rows are all one height and its columns all one width, so
    // the cells on screen follow from its scroll position.
    const cellCount = font ? cellsFor(font, charset, found).length : 0;
    useEffect(() => {
        const grid = gridRef.current;
        if (!grid) return;
        const measure = () => {
            const cell = grid.firstElementChild as HTMLElement | null;
            if (!cell || cell.offsetHeight === 0) return;
            const cols = Math.max(
                1,
                Math.round(grid.clientWidth / cell.offsetWidth),
            );
            const firstRow = Math.floor(grid.scrollTop / cell.offsetHeight);
            const rows = Math.ceil(grid.clientHeight / cell.offsetHeight);
            const first = firstRow * cols;
            const last = Math.min(cellCount, (firstRow + rows) * cols) - 1;
            setOnScreen((prev) =>
                prev[0] === first && prev[1] === last ? prev : [first, last],
            );
        };
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(grid);
        grid.addEventListener("scroll", measure, { passive: true });
        return () => {
            observer.disconnect();
            grid.removeEventListener("scroll", measure);
        };
    }, [cellCount]);

    /** Scrolls the grid so cell `index` is in the middle. */
    const scrollToCell = (index: number) => {
        const grid = gridRef.current;
        const cell = grid?.firstElementChild as HTMLElement | null;
        if (!grid || !cell) return;
        const cols = Math.max(
            1,
            Math.round(grid.clientWidth / cell.offsetWidth),
        );
        const top =
            Math.floor(index / cols) * cell.offsetHeight -
            (grid.clientHeight - cell.offsetHeight) / 2;
        grid.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    };

    if (!font) {
        return (
            <>
                <LeftColumn>{null}</LeftColumn>
                <Centre
                    toolbar={
                        <span className={sheet.view}>View 01 · Charset</span>
                    }
                >
                    <Empty>Checking…</Empty>
                </Centre>
            </>
        );
    }

    const cells = cellsFor(font, charset, found);
    const [descender, ascender] = verticalExtent(font);
    const missing = cells.filter((c) => !c.glyph && c.cp !== undefined);
    const inFont = cells.length - missing.length;
    const pickedSet = new Set(picked);
    const setName =
        charset === 0
            ? "All glyphs"
            : charset === -1
              ? `Search · ${query.trim() || "…"}`
              : CHARSETS[charset - 1].name;

    const toggle = (cell: Cell, range: boolean) => {
        if (cell.glyph) {
            setGlyph(cell.glyph.name, 2);
            return;
        }
        const cp = cell.cp;
        if (cp === undefined) return;
        if (range && picked.length > 0) {
            const from = picked[picked.length - 1];
            const [lo, hi] = from < cp ? [from, cp] : [cp, from];
            const span = missing
                .map((c) => c.cp as number)
                .filter((c) => c >= lo && c <= hi);
            setPicked([...new Set([...picked, ...span])]);
            return;
        }
        setPicked(
            pickedSet.has(cp)
                ? picked.filter((p) => p !== cp)
                : [...picked, cp],
        );
    };

    const toolbar = (
        <>
            <span className={sheet.view}>View 01 · Charset</span>
            <span className={styles.setName}>{setName}</span>
            <span className={styles.counts} title={`${inFont} in font`}>
                {charset !== 0
                    ? `${missing.length} missing`
                    : `${inFont} glyphs`}
            </span>
            <span className={sheet.spacer} />
            {charset !== 0 && (
                <>
                    <button
                        type="button"
                        className={sheet.toolButton}
                        onClick={() =>
                            setPicked(missing.map((c) => c.cp as number))
                        }
                    >
                        Pick missing
                    </button>
                    <button
                        type="button"
                        className={sheet.toolButton}
                        style={{ color: "var(--mid)" }}
                        onClick={() => setPicked([])}
                    >
                        Clear
                    </button>
                    <button
                        type="button"
                        className={sheet.toolButton}
                        disabled={picked.length === 0}
                        title={
                            picked.length === 0
                                ? "Pick missing characters first"
                                : "Name and insert the picked glyphs"
                        }
                        onClick={() => setAdding(true)}
                    >
                        Add {picked.length} glyph
                        {picked.length === 1 ? "" : "s"} →
                    </button>
                </>
            )}
        </>
    );

    return (
        <>
            <LeftColumn>
                <Find />
                <Charsets font={font} />
                <Legend />
            </LeftColumn>
            <Centre toolbar={toolbar}>
                {charset === -1 && cells.length === 0 && (
                    <Empty>
                        {query.trim()
                            ? `Nothing matches “${query.trim()}”.`
                            : "Type a name (“arrow”, “dagger”), a codepoint (U+2192), or paste characters."}
                    </Empty>
                )}
                <div
                    ref={gridRef}
                    className={styles.grid}
                    hidden={cells.length === 0}
                >
                    {cells.map((cell, i) => {
                        const isPicked =
                            cell.cp !== undefined && pickedSet.has(cell.cp);
                        const state = cell.glyph
                            ? "in"
                            : isPicked
                              ? "picked"
                              : "missing";
                        return (
                            <button
                                type="button"
                                key={cell.key}
                                className={styles.cell}
                                data-state={state}
                                title={cellTitle(cell, names)}
                                onPointerDown={(e) => {
                                    if (e.button !== 0 || e.shiftKey) return;
                                    if (charset === 0) return;
                                    // Starting on a picked cell unpicks.
                                    dragRef.current = {
                                        anchor: i,
                                        add: !isPicked,
                                        base: picked,
                                        moved: false,
                                    };
                                }}
                                onPointerEnter={(e) => {
                                    const d = dragRef.current;
                                    if (!d || !(e.buttons & 1)) return;
                                    d.moved = true;
                                    setPicked(
                                        dragPick(
                                            cells.map((c) => ({
                                                cp: c.cp,
                                                inFont: c.glyph !== undefined,
                                            })),
                                            d.anchor,
                                            i,
                                            d.add,
                                            d.base,
                                        ),
                                    );
                                }}
                                onClick={(e) => {
                                    if (dragRef.current?.moved) return;
                                    toggle(cell, e.shiftKey);
                                }}
                            >
                                <span className={styles.art}>
                                    {cell.glyph ? (
                                        <GlyphThumb
                                            className={styles.thumb}
                                            glyph={cell.glyph}
                                            ascender={ascender}
                                            descender={descender}
                                        />
                                    ) : (
                                        <span className={styles.char}>
                                            {sampleChar(cell.cp as number)}
                                        </span>
                                    )}
                                </span>
                                <span className={styles.meta}>
                                    <span>
                                        {charset === 0
                                            ? cell.glyph?.name
                                            : shortLabel(cell.cp as number)}
                                    </span>
                                    <Tag
                                        cell={cell}
                                        picked={isPicked}
                                        added={
                                            cell.glyph !== undefined &&
                                            added.includes(cell.glyph.name)
                                        }
                                    />
                                </span>
                            </button>
                        );
                    })}
                </div>
                {adding && (
                    <AddGlyphsModal
                        font={font}
                        codepoints={picked}
                        onClose={() => setAdding(false)}
                        onAdded={(names) => {
                            setPicked([]);
                            markAdded(names);
                        }}
                    />
                )}
                {charset !== 0 && cells.length > 0 && (
                    <Coverage
                        name={setName}
                        cells={cells}
                        picked={pickedSet}
                        onScreen={onScreen}
                        onToggle={toggle}
                        onJump={scrollToCell}
                    />
                )}
            </Centre>
        </>
    );
}

function cellTitle(cell: Cell, names: Names | null): string {
    const cp = cell.cp === undefined ? "" : charCode(cell.cp);
    const name =
        cell.cp === undefined || !names ? undefined : describe(cell.cp, names);
    return [cell.glyph?.name, cp, name].filter(Boolean).join(" · ");
}

/** The Unicode name table once `wanted` (it loads on first use, and stays
 * for the session), for cell titles. */
function useNames(wanted: boolean): Names | null {
    const [names, setNames] = useState<Names | null>(null);
    useEffect(() => {
        if (wanted && !names) void loadNames().then(setNames);
    }, [wanted, names]);
    return names;
}

/** Finds any character: by name, codepoint, or pasting it. */
function Find() {
    const query = useStore((s) => s.search);
    const charset = useStore((s) => s.charset);
    const setSearch = useStore((s) => s.setSearch);
    const latest = useRef(query);
    const run = (q: string) => {
        latest.current = q;
        // Shown at once; the results follow when the names have loaded.
        setSearch(q, useStore.getState().found);
        void loadNames().then((names) => {
            if (latest.current === q) setSearch(q, search(q, names));
        });
    };
    return (
        <Section title="Find a character" flush>
            <div className={styles.find}>
                <input
                    type="search"
                    className={styles.findInput}
                    value={query}
                    placeholder="name, U+hex (U+0030 U+FE00), or paste"
                    aria-label="Find a character by name, codepoint, or the character itself"
                    spellCheck={false}
                    onFocus={() => {
                        void loadNames();
                        if (charset !== -1) run(query);
                    }}
                    onChange={(e) => run(e.target.value)}
                />
            </div>
        </Section>
    );
}

function Tag({
    cell,
    picked,
    added,
}: {
    cell: Cell;
    picked: boolean;
    added: boolean;
}) {
    if (cell.glyph?.errors) {
        return <span style={{ color: "var(--err)" }}>ERR</span>;
    }
    if (cell.glyph?.components) {
        return <span style={{ color: "var(--con)" }}>CMP</span>;
    }
    if (added) return <span style={{ color: "var(--acc)" }}>NEW</span>;
    if (cell.glyph && !cell.glyph.ink && !cell.glyph.components) {
        return <span style={{ color: "var(--faint)" }}>EMPTY</span>;
    }
    if (picked) return <span style={{ color: "var(--acc)" }}>✓</span>;
    return null;
}

/** Pixels a coverage bar takes: 3 wide and a 1px gap. */
const BAR_PX = 4;

/**
 * The set's coverage, one bar per character. When there are more than fit,
 * it shows a window of them centred on what the grid shows, and a track
 * below with that window's place in the whole set: clicking the track
 * scrolls the grid there. Bars the grid shows are full strength.
 */
function Coverage({
    name,
    cells,
    picked,
    onScreen,
    onToggle,
    onJump,
}: {
    name: string;
    cells: Cell[];
    picked: Set<number>;
    onScreen: [number, number];
    onToggle: (cell: Cell, range: boolean) => void;
    onJump: (index: number) => void;
}) {
    const barsRef = useRef<HTMLDivElement>(null);
    const [capacity, setCapacity] = useState(Number.POSITIVE_INFINITY);
    useEffect(() => {
        const bars = barsRef.current;
        if (!bars) return;
        const observer = new ResizeObserver(([entry]) =>
            setCapacity(
                Math.max(1, Math.floor((entry.contentRect.width + 1) / BAR_PX)),
            ),
        );
        observer.observe(bars);
        return () => observer.disconnect();
    }, []);

    const n = cells.length;
    const inFont = cells.filter((c) => c.glyph).length;
    const [start, end] = coverageWindow(
        n,
        capacity,
        (onScreen[0] + onScreen[1]) / 2,
    );
    const windowed = end - start < n;
    const shown = cells.slice(start, end);
    return (
        <div className={styles.coverage}>
            <div className={styles.coverageHead}>
                <span>Coverage · {name}</span>
                <span className={styles.coverageCount}>
                    {inFont} / {n}
                </span>
            </div>
            <div ref={barsRef} className={styles.bars}>
                {shown.map((cell, j) => {
                    const i = start + j;
                    return (
                        <button
                            type="button"
                            key={cell.key}
                            className={styles.bar}
                            data-state={
                                cell.glyph
                                    ? "in"
                                    : cell.cp !== undefined &&
                                        picked.has(cell.cp)
                                      ? "picked"
                                      : "missing"
                            }
                            data-off={
                                i < onScreen[0] || i > onScreen[1] || undefined
                            }
                            title={`U+${cell.key}`}
                            onClick={(e) => {
                                onToggle(cell, e.shiftKey);
                                if (i < onScreen[0] || i > onScreen[1]) {
                                    onJump(i);
                                }
                            }}
                        />
                    );
                })}
            </div>
            {windowed && (
                // biome-ignore lint/a11y/useKeyWithClickEvents: the grid scrolls by keyboard; this is a shortcut for the mouse
                // biome-ignore lint/a11y/noStaticElementInteractions: as above
                <div
                    className={styles.track}
                    title="Where these bars are in the set · click to go there"
                    onClick={(e) => {
                        const box = e.currentTarget.getBoundingClientRect();
                        const at = (e.clientX - box.left) / box.width;
                        onJump(Math.floor(at * n));
                    }}
                >
                    <span
                        className={styles.trackThumb}
                        style={{
                            left: `${(start / n) * 100}%`,
                            width: `${((end - start) / n) * 100}%`,
                        }}
                    />
                </div>
            )}
            <div className={styles.coverageEnds}>
                <span>U+{shown[0]?.key}</span>
                {windowed && (
                    <span>
                        {start + 1}–{end} of {n}
                    </span>
                )}
                <span>U+{shown[shown.length - 1]?.key}</span>
            </div>
        </div>
    );
}

function Charsets({ font }: { font: FontData }) {
    const charset = useStore((s) => s.charset);
    const setCharset = useStore((s) => s.setCharset);
    const map = glyphsByCodepoint(font);
    const sets = [
        {
            name: "All glyphs",
            n: font.glyphs.length,
            total: font.glyphs.length,
        },
        ...CHARSETS.map(({ name, codepoints: cps }) => {
            return {
                name,
                n: cps.filter((cp) => map.has(cp)).length,
                total: cps.length,
            };
        }),
    ];
    return (
        <Section title="Character sets" flush>
            {sets.map((set, i) => (
                <button
                    type="button"
                    key={set.name}
                    className={styles.set}
                    data-on={charset === i || undefined}
                    onClick={() => setCharset(i)}
                >
                    <span className={styles.setRow}>
                        <span>{set.name}</span>
                        <span className={styles.setCount}>
                            {i === 0 ? set.n : `${set.n} / ${set.total}`}
                        </span>
                    </span>
                    {i > 0 && (
                        <span className={styles.setBar}>
                            <span
                                style={{
                                    width: `${(set.n / set.total) * 100}%`,
                                }}
                            />
                        </span>
                    )}
                </button>
            ))}
        </Section>
    );
}

function Legend() {
    return (
        <>
            <Section title="Legend">
                <div className={styles.legend}>
                    <span>
                        <i className={styles.swatch} data-state="in">
                            A
                        </i>
                        In font · click to open
                    </span>
                    <span>
                        <i className={styles.swatch} data-state="missing" />
                        Missing · click or drag to pick
                    </span>
                    <span>
                        <i className={styles.swatch} data-state="picked" />
                        Picked for adding
                    </span>
                </div>
            </Section>
            <p className={`note ${styles.aglfn}`}>
                New glyphs get AGLFN names (<code>uniXXXX</code> fallback) and{" "}
                the font's most common <code>advance</code>. Adding them is a
                single undo step.
            </p>
        </>
    );
}
