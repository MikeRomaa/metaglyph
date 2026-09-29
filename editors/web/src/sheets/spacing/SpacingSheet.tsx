import type { FontData, GlyphInfo } from "../../engine/types.ts";
import {
    fmt,
    glyphsForText,
    spacing,
    verticalExtent,
} from "../../font/lookup.ts";
import { useStore } from "../../state/store.ts";
import {
    Ball,
    Centre,
    Empty,
    LeftColumn,
    Section,
    sheet,
} from "../../ui/Sheet.tsx";
import styles from "./SpacingSheet.module.css";

/** The active glyph and the next few that have characters. */
function defaultText(font: FontData, active?: GlyphInfo): string {
    const withChars = font.glyphs.filter((g) => g.codepoints.length > 0);
    const start = Math.max(0, active ? withChars.indexOf(active) : 0);
    return withChars
        .slice(start, start + 4)
        .map((g) => String.fromCodePoint(g.codepoints[0]))
        .join("");
}

function declares(glyph: GlyphInfo): string {
    const { advance, lsb, rsb } = glyph.fields;
    return [advance && "advance", lsb && "lsb", rsb && "rsb"]
        .filter(Boolean)
        .join(" + ");
}

export function SpacingSheet() {
    const font = useStore((s) => s.font);
    const name = useStore((s) => s.glyph);
    const text = useStore((s) => s.spacingText);
    const setText = useStore((s) => s.setSpacingText);
    const setGlyph = useStore((s) => s.setGlyph);
    const active = font?.glyphs.find((g) => g.name === name);
    const fallback = font ? defaultText(font, active) : "";

    const toolbar = (
        <>
            <span className={sheet.view}>View 03 · Spacing</span>
            <span className={sheet.toolLabel}>String</span>
            <input
                className={sheet.toolInput}
                value={text}
                placeholder={fallback}
                spellCheck={false}
                onChange={(e) => setText(e.target.value)}
            />
        </>
    );

    if (!font) {
        return (
            <>
                <LeftColumn>{null}</LeftColumn>
                <Centre toolbar={toolbar}>
                    <Empty>Checking…</Empty>
                </Centre>
            </>
        );
    }

    const row = glyphsForText(font, text || fallback);

    return (
        <>
            <LeftColumn>
                {active && <Fields glyph={active} />}
                <MetricLines font={font} />
                <FontInfoForm />
            </LeftColumn>
            <Centre toolbar={toolbar}>
                <div className={styles.row}>
                    {row.length > 0 ? (
                        <SpacingRow
                            font={font}
                            row={row}
                            active={name}
                            onPick={setGlyph}
                        />
                    ) : (
                        <Empty>
                            No glyph in the font sets these characters.
                        </Empty>
                    )}
                </div>
                <div className={styles.bomWrap}>
                    <div className={sheet.bomHead}>
                        <span className={sheet.bomTitle}>
                            Bill of materials — spacing
                        </span>
                        <span className={sheet.aside}>
                            DWG 03 · {font.glyphs.length} ITEMS
                        </span>
                    </div>
                    <div className={sheet.bom}>
                        <div
                            className={`${sheet.bomRow} ${sheet.bomHeader} ${styles.cols}`}
                        >
                            <span>Item</span>
                            <span>Glyph</span>
                            <span>Advance</span>
                            <span>LSB</span>
                            <span>RSB</span>
                            <span>Declares</span>
                        </div>
                        {font.glyphs.map((g, i) => {
                            const sp = spacing(g);
                            const on = g.name === name;
                            return (
                                <button
                                    type="button"
                                    key={g.name}
                                    className={`${sheet.bomRow} ${styles.cols}`}
                                    data-on={on || undefined}
                                    onClick={() => setGlyph(g.name)}
                                >
                                    <span>
                                        <Ball n={i + 1} on={on} />
                                    </span>
                                    <span style={{ fontWeight: 500 }}>
                                        {g.name}
                                    </span>
                                    <span style={{ color: "var(--mid)" }}>
                                        {g.fields.advance
                                            ? `${g.fields.advance} · `
                                            : ""}
                                        {fmt(sp.advance)}
                                    </span>
                                    <span className={sheet.num}>
                                        {fmt(sp.lsb)}
                                    </span>
                                    <span className={sheet.num}>
                                        {fmt(sp.rsb)}
                                    </span>
                                    <span style={{ color: "var(--mid)" }}>
                                        {declares(g)}
                                    </span>
                                </button>
                            );
                        })}
                    </div>
                </div>
            </Centre>
        </>
    );
}

function SpacingRow({
    font,
    row,
    active,
    onPick,
}: {
    font: FontData;
    row: GlyphInfo[];
    active: string | null;
    onPick: (name: string) => void;
}) {
    const [descender, ascender] = verticalExtent(font);
    const height = ascender - descender;
    const fs = height * 0.045;
    const pad = height * 0.08;
    const dimY = -descender + fs * 3.2;
    let x = 0;
    const cols = row.map((glyph, i) => {
        const x0 = x;
        const advance = glyph.advance ?? 0;
        x += advance;
        return { glyph, x0, x1: x0 + advance, i };
    });
    const total = x;
    const Y = (y: number) => -y;
    const vb = [
        -pad * 2.5,
        Y(ascender) - pad,
        total + pad * 3.5,
        height + pad + fs * 5,
    ];

    return (
        <svg
            className={styles.svg}
            viewBox={vb.join(" ")}
            preserveAspectRatio="xMidYMid meet"
        >
            <title>Spacing</title>
            {font.metrics
                .filter((m) => m.y !== undefined)
                .map((m) => (
                    <g key={m.name}>
                        <line
                            x1={vb[0]}
                            x2={vb[0] + vb[2]}
                            y1={Y(m.y as number)}
                            y2={Y(m.y as number)}
                            className={
                                m.name === "baseline"
                                    ? styles.baseline
                                    : styles.metric
                            }
                        />
                        <text
                            x={vb[0] + fs * 0.3}
                            y={Y(m.y as number) - fs * 0.35}
                            fontSize={fs}
                            className={styles.metricLabel}
                        >
                            {m.name}
                        </text>
                    </g>
                ))}
            {cols.map(({ glyph, x0, x1, i }) => {
                const on = glyph.name === active;
                const sp = spacing(glyph);
                const mid = (x0 + x1) / 2;
                return (
                    // biome-ignore lint/a11y/useSemanticElements: SVG has no <button>; this <g> is a keyboard-operable one
                    <g
                        key={i}
                        className={styles.col}
                        data-on={on || undefined}
                        role="button"
                        tabIndex={0}
                        aria-label={`Select ${glyph.name}`}
                        onClick={() => onPick(glyph.name)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ")
                                onPick(glyph.name);
                        }}
                    >
                        <rect
                            x={x0}
                            y={Y(ascender)}
                            width={x1 - x0}
                            height={height}
                            className={styles.colBg}
                        />
                        <line
                            x1={x0}
                            x2={x0}
                            y1={Y(ascender) - pad * 0.5}
                            y2={dimY}
                            className={styles.guide}
                        />
                        <line
                            x1={x1}
                            x2={x1}
                            y1={Y(ascender) - pad * 0.5}
                            y2={dimY}
                            className={styles.guide}
                        />
                        <path
                            d={glyph.outline}
                            transform={`translate(${x0} 0) scale(1 -1)`}
                            className={styles.ink}
                        />
                        <text
                            x={x0 + fs * 0.3}
                            y={-descender - fs * 0.6}
                            fontSize={fs}
                            className={styles.bearing}
                        >
                            {fmt(sp.lsb)}
                        </text>
                        <text
                            x={x1 - fs * 0.3}
                            y={-descender - fs * 0.6}
                            fontSize={fs}
                            textAnchor="end"
                            className={styles.bearing}
                        >
                            {fmt(sp.rsb)}
                        </text>
                        <line
                            x1={x0}
                            x2={x1}
                            y1={dimY}
                            y2={dimY}
                            className={styles.dim}
                        />
                        <path
                            d={`M${x0} ${dimY} l${fs * 0.7} ${-fs * 0.28} v${fs * 0.56} z M${x1} ${dimY} l${-fs * 0.7} ${-fs * 0.28} v${fs * 0.56} z`}
                            className={styles.arrow}
                        />
                        <rect
                            x={mid - fs * 2.6}
                            y={dimY - fs * 0.7}
                            width={fs * 5.2}
                            height={fs * 1.4}
                            className={styles.dimBox}
                        />
                        <text
                            x={mid}
                            y={dimY + fs * 0.35}
                            fontSize={fs}
                            textAnchor="middle"
                            className={styles.name}
                        >
                            {glyph.name}
                        </text>
                    </g>
                );
            })}
        </svg>
    );
}

function Fields({ glyph }: { glyph: GlyphInfo }) {
    const sp = spacing(glyph);
    const rows: [string, string | undefined, number | undefined][] = [
        ["advance", glyph.fields.advance, sp.advance],
        ["lsb", glyph.fields.lsb, sp.lsb],
        ["rsb", glyph.fields.rsb, sp.rsb],
    ];
    return (
        <Section
            title={`Spacing · ${glyph.name}`}
            aside={`declares ${declares(glyph)}`}
            flush
        >
            {rows.map(([key, src, value]) => (
                <div key={key} className={`${sheet.row} ${styles.fieldRow}`}>
                    <span style={{ fontWeight: 500 }}>{key}</span>
                    <span
                        style={{ color: src ? "var(--ink)" : "var(--faint)" }}
                    >
                        {src ?? "derived"}
                    </span>
                    <span className={sheet.num}>{fmt(value)}</span>
                </div>
            ))}
        </Section>
    );
}

function MetricLines({ font }: { font: FontData }) {
    const select = useStore((s) => s.select);
    return (
        <Section title="Metric lines" flush>
            {font.metrics.map((m) => (
                <button
                    type="button"
                    key={m.name}
                    className={`${sheet.row} ${styles.metricRow}`}
                    onClick={() =>
                        select({
                            kind: "metric",
                            name: m.name,
                            span: m.span,
                            origin: "canvas",
                        })
                    }
                >
                    <span style={{ fontWeight: 500 }}>{m.name}</span>
                    <span style={{ color: "var(--mid)" }}>{m.expr}</span>
                    <span className={sheet.num}>{fmt(m.y)}</span>
                </button>
            ))}
        </Section>
    );
}

function FontInfoForm() {
    const info = useStore((s) => s.lastGood?.font);
    const rows: [string, string | undefined, boolean][] = [
        ["Name", info?.name, true],
        ["Version", info?.version, true],
        ["Designer", info?.designer, true],
        ["Foundry", info?.foundry, true],
        ["Em", info?.em?.toString(), false],
    ];
    return (
        <Section title="Font info" flush>
            {rows.map(([key, value, str]) => (
                <div key={key} className={`${sheet.row} ${styles.infoRow}`}>
                    <span className={sheet.key}>{key}</span>
                    <span
                        title={value}
                        style={{
                            color:
                                value === undefined
                                    ? "var(--faint)"
                                    : str
                                      ? "var(--str)"
                                      : "var(--ink)",
                        }}
                    >
                        {value === undefined ? "—" : str ? `"${value}"` : value}
                    </span>
                </div>
            ))}
        </Section>
    );
}
