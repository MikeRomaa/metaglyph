import type { FontData, GlyphInfo } from "../../engine/types.ts";
import { BLOCKS, codepoints } from "../../font/blocks.ts";
import { glyphsByCodepoint, hex, verticalExtent } from "../../font/lookup.ts";
import { useStore } from "../../state/store.ts";
import { GlyphThumb } from "../../ui/GlyphThumb.tsx";
import { Centre, Empty, LeftColumn, Section, sheet } from "../../ui/Sheet.tsx";
import styles from "./GlyphsSheet.module.css";

/** One cell: a glyph in the font, or a codepoint it lacks. */
interface Cell {
    key: string;
    cp?: number;
    glyph?: GlyphInfo;
}

function cellsFor(font: FontData, charset: number): Cell[] {
    if (charset === 0) {
        return font.glyphs.map((glyph) => ({
            key: glyph.name,
            cp: glyph.codepoints[0],
            glyph,
        }));
    }
    const map = glyphsByCodepoint(font);
    return codepoints(BLOCKS[charset - 1]).map((cp) => ({
        key: hex(cp),
        cp,
        glyph: map.get(cp),
    }));
}

function char(cp: number) {
    return cp === 0x20 ? "␣" : String.fromCodePoint(cp);
}

export function GlyphsSheet() {
    const font = useStore((s) => s.font);
    const charset = useStore((s) => s.charset);
    const picked = useStore((s) => s.picked);
    const setPicked = useStore((s) => s.setPicked);
    const setGlyph = useStore((s) => s.setGlyph);

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

    const cells = cellsFor(font, charset);
    const [descender, ascender] = verticalExtent(font);
    const missing = cells.filter((c) => !c.glyph && c.cp !== undefined);
    const inFont = cells.length - missing.length;
    const pickedSet = new Set(picked);
    const setName = charset === 0 ? "All glyphs" : BLOCKS[charset - 1].name;

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
            <span className={styles.counts}>
                {inFont} in font
                {charset > 0 && ` · ${missing.length} missing`}
            </span>
            <span className={sheet.spacer} />
            {charset > 0 && (
                <>
                    <button
                        type="button"
                        className={sheet.toolButton}
                        onClick={() =>
                            setPicked(missing.map((c) => c.cp as number))
                        }
                    >
                        Pick all missing
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
                        disabled
                        title="Adding glyphs arrives in W8"
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
                <Charsets font={font} />
                <Legend />
            </LeftColumn>
            <Centre toolbar={toolbar}>
                <div className={styles.grid}>
                    {cells.map((cell) => {
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
                                title={
                                    cell.glyph
                                        ? `${cell.glyph.name}${cell.cp !== undefined ? ` · U+${hex(cell.cp)}` : ""}`
                                        : `U+${hex(cell.cp as number)}`
                                }
                                onClick={(e) => toggle(cell, e.shiftKey)}
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
                                            {char(cell.cp as number)}
                                        </span>
                                    )}
                                </span>
                                <span className={styles.meta}>
                                    <span>
                                        {charset === 0
                                            ? cell.glyph?.name
                                            : hex(cell.cp as number)}
                                    </span>
                                    <Tag cell={cell} picked={isPicked} />
                                </span>
                            </button>
                        );
                    })}
                </div>
                {charset > 0 && (
                    <Coverage
                        name={setName}
                        cells={cells}
                        picked={pickedSet}
                        onToggle={toggle}
                    />
                )}
            </Centre>
        </>
    );
}

function Tag({ cell, picked }: { cell: Cell; picked: boolean }) {
    if (cell.glyph?.errors) {
        return <span style={{ color: "var(--err)" }}>ERR</span>;
    }
    if (cell.glyph?.components) {
        return <span style={{ color: "var(--con)" }}>CMP</span>;
    }
    if (cell.glyph && !cell.glyph.ink && !cell.glyph.components) {
        return <span style={{ color: "var(--faint)" }}>EMPTY</span>;
    }
    if (picked) return <span style={{ color: "var(--acc)" }}>✓</span>;
    return null;
}

function Coverage({
    name,
    cells,
    picked,
    onToggle,
}: {
    name: string;
    cells: Cell[];
    picked: Set<number>;
    onToggle: (cell: Cell, range: boolean) => void;
}) {
    const inFont = cells.filter((c) => c.glyph).length;
    return (
        <div className={styles.coverage}>
            <div className={styles.coverageHead}>
                <span>Coverage · {name}</span>
                <span className={styles.coverageCount}>
                    {inFont} / {cells.length}
                </span>
            </div>
            <div className={styles.bars}>
                {cells.map((cell) => (
                    <button
                        type="button"
                        key={cell.key}
                        className={styles.bar}
                        data-state={
                            cell.glyph
                                ? "in"
                                : cell.cp !== undefined && picked.has(cell.cp)
                                  ? "picked"
                                  : "missing"
                        }
                        title={`U+${cell.key}`}
                        onClick={(e) => onToggle(cell, e.shiftKey)}
                    />
                ))}
            </div>
            <div className={styles.coverageEnds}>
                <span>U+{cells[0]?.key}</span>
                <span>U+{cells[cells.length - 1]?.key}</span>
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
        ...BLOCKS.map((b) => {
            const cps = codepoints(b);
            return {
                name: b.name,
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
                        Missing · click to pick
                    </span>
                    <span>
                        <i className={styles.swatch} data-state="picked" />
                        Picked for adding
                    </span>
                </div>
            </Section>
            <p className={`note ${styles.aglfn}`}>
                New glyphs get AGLFN names (<code>uniXXXX</code> fallback) and{" "}
                <code>rsb: 0</code>. Adding them is a single undo step.
            </p>
        </>
    );
}
