import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { Edge, FontData, GlyphInfo, Op } from "../../engine/types.ts";
import {
    fmt,
    glyphsForText,
    spacing,
    verticalExtent,
} from "../../font/lookup.ts";
import { EditGesture } from "../../state/gesture.ts";
import {
    FONT_FIELDS,
    type SpacingField,
    setFontField,
    spacingOp,
    typeMetric,
    typeSpacing,
} from "../../state/spacing.ts";
import { useStore } from "../../state/store.ts";
import { ExprField } from "../../ui/ExprField.tsx";
import {
    Ball,
    Centre,
    Empty,
    LeftColumn,
    Section,
    sheet,
} from "../../ui/Sheet.tsx";
import styles from "./SpacingSheet.module.css";

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

    const toolbar = (
        <>
            <span className={sheet.view}>View 03 · Spacing</span>
            <span className={sheet.toolLabel}>String</span>
            <input
                className={sheet.toolInput}
                value={text}
                placeholder="all glyphs"
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

    // With no string, every glyph, as a carousel on the active one.
    const row = text ? glyphsForText(font, text) : font.glyphs;

    return (
        <>
            <LeftColumn>
                {active && <Fields key={active.name} glyph={active} />}
                <MetricLines font={font} />
                <FontInfoForm />
            </LeftColumn>
            <Centre toolbar={toolbar}>
                <div className={styles.row}>
                    {row.length > 0 ? (
                        <SpacingRow
                            font={font}
                            row={row}
                            carousel={!text}
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

// ---------------------------------------------------------------------
// The row

/** What a drag moves: a glyph's guide or its ink (by its index in the
 * row), or a metric line. */
type Target =
    | { kind: "spacing"; index: number; glyph: string; edge: Edge }
    | { kind: "metric"; name: string };

interface Drag {
    target: Target;
    /** Pointer positions in SVG coordinates (y down). */
    start: [number, number];
    last: [number, number];
    /** Null until the engine has pinned the start text. */
    gesture: EditGesture | null;
    /** The op for the latest pointer position, from the start state. */
    op: ((at: [number, number]) => Op | null) | null;
    ended: boolean;
}

/** While dragging, the drawing keeps its frame: the viewBox and the
 * carousel stay, and the dragged glyph's ink stays put while its guides
 * move (spec §12.1). */
interface Frozen {
    target: Target;
    viewBox: number[];
    centre: number;
    /** Row index and placed ink x at the start, for a guide drag. */
    pin: { index: number; x: number } | null;
}

/** Where a glyph's ink starts in the row, or its origin with no ink. */
function inkX(x0: number, glyph: GlyphInfo): number {
    return x0 + (glyph.ink ? glyph.ink[0] + (glyph.shift ?? 0) : 0);
}

/**
 * The row of glyphs. A typed string is fitted whole; the whole font is a
 * carousel centred on the active glyph, sliding when it changes. Only the
 * active glyph's guides drag: clicking any other glyph, sidebearings
 * included, selects it first.
 */
function SpacingRow({
    font,
    row,
    carousel,
    active,
    onPick,
}: {
    font: FontData;
    row: GlyphInfo[];
    carousel: boolean;
    active: string | null;
    onPick: (name: string) => void;
}) {
    const svgRef = useRef<SVGSVGElement>(null);
    const dragRef = useRef<Drag | null>(null);
    const [frozen, setFrozen] = useState<Frozen | null>(null);
    const [callout, setCallout] = useState<{ x: number; text: string } | null>(
        null,
    );
    /** The drawing's size in pixels, for the carousel's width. */
    const [size, setSize] = useState<[number, number] | null>(null);

    useEffect(() => {
        const svg = svgRef.current;
        if (!svg) return;
        const observer = new ResizeObserver(([entry]) => {
            const { width, height } = entry.contentRect;
            if (width > 0 && height > 0) setSize([width, height]);
        });
        observer.observe(svg);
        return () => observer.disconnect();
    }, []);

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
    // Room above the ascender for the drag callout.
    const top = Y(ascender) - pad - fs * 2.4;
    const vbHeight = -descender + fs * 5 - top;
    const vbWidth =
        carousel && size ? (vbHeight * size[0]) / size[1] : total + pad * 3.5;
    const anchored = cols.find((c) => c.glyph.name === active) ?? cols[0];
    const liveCentre = carousel ? (anchored.x0 + anchored.x1) / 2 : 0;
    const viewBox = frozen?.viewBox ?? [
        carousel ? -vbWidth / 2 : -pad * 2.5,
        top,
        vbWidth,
        vbHeight,
    ];
    const vb = viewBox;
    const centre = frozen?.centre ?? liveCentre;
    const pinned = frozen?.pin && cols[frozen.pin.index];
    const offset =
        pinned && frozen.pin ? frozen.pin.x - inkX(pinned.x0, pinned.glyph) : 0;
    // The carousel draws only what can slide into view.
    const shown = carousel
        ? cols.filter((c) => c.x1 > centre - vbWidth && c.x0 < centre + vbWidth)
        : cols;

    /** The pointer in SVG user coordinates. */
    const toSvg = (e: { clientX: number; clientY: number }) => {
        const svg = svgRef.current;
        const m = svg?.getScreenCTM();
        if (!svg || !m) return null;
        const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(
            m.inverse(),
        );
        return [p.x, p.y] as [number, number];
    };

    const push = (d: Drag) => {
        if (!d.gesture || !d.op || d.ended) return;
        const op = d.op(d.last);
        if (!op) return;
        const at = d.last;
        d.gesture.update(op, (text) =>
            setCallout(text ? { x: at[0], text } : null),
        );
    };

    const finish = async (d: Drag, cancel: boolean) => {
        d.ended = true;
        await d.gesture?.end(cancel);
        if (dragRef.current === d) dragRef.current = null;
        setFrozen(null);
        setCallout(null);
    };

    /** The op a drag's pointer position makes, from the start state. */
    const opFor = (target: Target): Drag["op"] => {
        const s = useStore.getState();
        const font = s.font;
        if (!font) return null;
        if (target.kind === "metric") {
            const m = font.metrics.find((x) => x.name === target.name);
            if (!m) return null;
            const span = m.span;
            return (at) => {
                const d = dragRef.current;
                if (!d) return null;
                // SVG y runs down.
                const delta = Math.round(d.start[1] - at[1]);
                return {
                    op: "addConstant",
                    span,
                    name: "y",
                    delta,
                    em: font.em,
                };
            };
        }
        const g = font.glyphs.find((x) => x.name === target.glyph);
        if (!g) return null;
        return (at) => {
            const d = dragRef.current;
            if (!d) return null;
            const dx = Math.round(at[0] - d.start[0]);
            // The bearing grows as the origin guide moves left; the ink
            // moves with the pointer.
            const delta = target.edge === "left" ? -dx : dx;
            return spacingOp(g, target.edge, delta);
        };
    };

    const begin = (e: PointerEvent<SVGElement>, target: Target) => {
        if (e.button !== 0 || dragRef.current) return;
        const at = toSvg(e);
        if (!at) return;
        e.stopPropagation();
        e.currentTarget.setPointerCapture(e.pointerId);
        const d: Drag = {
            target,
            start: at,
            last: at,
            gesture: null,
            op: null,
            ended: false,
        };
        dragRef.current = d;
        if (target.kind === "spacing") {
            const col = cols[target.index];
            setFrozen({
                target,
                viewBox,
                centre,
                // An ink drag moves the ink and keeps the frame.
                pin:
                    target.edge === "ink"
                        ? null
                        : {
                              index: target.index,
                              x: offset + inkX(col.x0, col.glyph),
                          },
            });
        } else {
            const m = font.metrics.find((x) => x.name === target.name);
            if (m) {
                useStore.getState().select({
                    kind: "metric",
                    name: m.name,
                    span: m.span,
                    origin: "canvas",
                });
            }
            setFrozen({ target, viewBox, centre, pin: null });
        }
        const label = target.kind === "metric" ? "metric_drag" : "guide_drag";
        void EditGesture.begin(label).then((gesture) => {
            if (!gesture) {
                void finish(d, true);
                return;
            }
            d.gesture = gesture;
            d.op = opFor(target);
            if (d.ended) void gesture.end();
            else push(d);
        });
    };

    const move = (e: PointerEvent) => {
        const d = dragRef.current;
        const at = d && toSvg(e);
        if (!d || !at) return;
        d.last = at;
        push(d);
    };

    const end = (e: PointerEvent) => {
        const d = dragRef.current;
        if (!d) return;
        e.stopPropagation();
        void finish(d, false);
    };

    // Escape cancels a drag, ahead of the editor's own Escape.
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            const d = dragRef.current;
            if (e.key !== "Escape" || !d) return;
            e.stopImmediatePropagation();
            void finish(d, true);
        };
        window.addEventListener("keydown", onKey, true);
        return () => window.removeEventListener("keydown", onKey, true);
    });

    const draggingTarget = frozen?.target;

    return (
        <svg
            ref={svgRef}
            className={styles.svg}
            viewBox={vb.join(" ")}
            preserveAspectRatio="xMidYMid meet"
            onPointerMove={move}
            onPointerUp={end}
            onPointerCancel={(e) => {
                const d = dragRef.current;
                if (d) {
                    e.stopPropagation();
                    void finish(d, true);
                }
            }}
        >
            <title>Spacing</title>
            {font.metrics
                .filter((m) => m.y !== undefined)
                .map((m) => {
                    const y = Y(m.y as number);
                    const locked = m.name === "baseline";
                    const on =
                        draggingTarget?.kind === "metric" &&
                        draggingTarget.name === m.name;
                    return (
                        <g key={m.name} data-on={on || undefined}>
                            <line
                                x1={vb[0]}
                                x2={vb[0] + vb[2]}
                                y1={y}
                                y2={y}
                                className={
                                    locked ? styles.baseline : styles.metric
                                }
                            />
                            <text
                                x={vb[0] + fs * 0.3}
                                y={y - fs * 0.35}
                                fontSize={fs}
                                className={styles.metricLabel}
                            >
                                {m.name}
                            </text>
                            {!locked && (
                                <rect
                                    x={vb[0]}
                                    y={y - fs * 0.3}
                                    width={fs * 8}
                                    height={fs * 0.6}
                                    className={styles.metricHit}
                                    onPointerDown={(e) =>
                                        begin(e, {
                                            kind: "metric",
                                            name: m.name,
                                        })
                                    }
                                >
                                    <title>{`Drag to move ${m.name}.y`}</title>
                                </rect>
                            )}
                        </g>
                    );
                })}
            <g
                className={styles.slide}
                data-still={frozen ? true : undefined}
                style={{ transform: `translateX(${offset - centre}px)` }}
            >
                {shown.map(({ glyph, x0, x1, i }) => {
                    const on = glyph.name === active;
                    const sp = spacing(glyph);
                    const mid = (x0 + x1) / 2;
                    const guideOn = (edge: Edge) =>
                        draggingTarget?.kind === "spacing" &&
                        draggingTarget.index === i &&
                        draggingTarget.edge === edge;
                    return (
                        // biome-ignore lint/a11y/useSemanticElements: SVG has no <button>; this <g> is a keyboard-operable one
                        <g
                            key={i}
                            className={styles.col}
                            data-on={on || undefined}
                            role="button"
                            tabIndex={0}
                            aria-label={`Select ${glyph.name}`}
                            data-ink={on || undefined}
                            onClick={() => onPick(glyph.name)}
                            onPointerDown={(e) => {
                                // The active glyph's ink drags within its
                                // advance; others are only picked.
                                if (on) {
                                    begin(e, {
                                        kind: "spacing",
                                        index: i,
                                        glyph: glyph.name,
                                        edge: "ink",
                                    });
                                }
                            }}
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
                                data-drag={guideOn("left") || undefined}
                            />
                            <line
                                x1={x1}
                                x2={x1}
                                y1={Y(ascender) - pad * 0.5}
                                y2={dimY}
                                className={styles.guide}
                                data-drag={guideOn("right") || undefined}
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
                {/* The active glyph's guide handles, over its neighbours:
                    a sidebearing only drags once its glyph is picked. */}
                {shown
                    .filter(({ glyph }) => glyph.name === active)
                    .flatMap(({ glyph, x0, x1, i }) =>
                        (["left", "right"] as const).map((edge) => (
                            <rect
                                key={`${i}${edge}`}
                                x={(edge === "left" ? x0 : x1) - fs * 0.9}
                                y={Y(ascender) - pad * 0.5}
                                width={fs * 1.8}
                                height={dimY - Y(ascender) + pad * 0.5}
                                className={styles.guideHit}
                                onPointerDown={(e) =>
                                    begin(e, {
                                        kind: "spacing",
                                        index: i,
                                        glyph: glyph.name,
                                        edge,
                                    })
                                }
                            >
                                <title>
                                    {edge === "left"
                                        ? `Drag the origin guide of ${glyph.name}`
                                        : `Drag the advance guide of ${glyph.name}`}
                                </title>
                            </rect>
                        )),
                    )}
            </g>
            {callout && (
                <Callout
                    x={callout.x}
                    y={vb[1]}
                    fs={fs}
                    text={callout.text}
                    bounds={[vb[0], vb[0] + vb[2]]}
                />
            )}
        </svg>
    );
}

/** The live callout above a drag, hanging from `y` (the drawing's top):
 * what the drag writes to the source. */
function Callout({
    x,
    y,
    fs,
    text,
    bounds,
}: {
    x: number;
    y: number;
    fs: number;
    text: string;
    bounds: [number, number];
}) {
    const size = fs * 0.9;
    // Plex Mono advances 0.6 em.
    const width = text.length * size * 0.6 + size * 1.2;
    const left = Math.max(
        bounds[0],
        Math.min(x - width / 2, bounds[1] - width),
    );
    return (
        <g className={styles.callout} pointerEvents="none">
            <rect
                x={left}
                y={y + size * 0.4}
                width={width}
                height={size * 1.6}
                className={styles.calloutBox}
            />
            <text
                x={left + size * 0.6}
                y={y + size * 1.55}
                fontSize={size}
                className={styles.calloutText}
            >
                {text}
            </text>
        </g>
    );
}

// ---------------------------------------------------------------------
// The left column

const SPACING_FIELDS: SpacingField[] = ["advance", "lsb", "rsb"];

/** The glyph's spacing fields, typed in place (plan 5, §1.6). A field
 * declared since the glyph was opened is highlighted. */
function Fields({ glyph }: { glyph: GlyphInfo }) {
    const sp = spacing(glyph);
    const [initial] = useState(
        () => new Set(SPACING_FIELDS.filter((k) => glyph.fields[k])),
    );
    return (
        <Section
            title={`Spacing · ${glyph.name}`}
            aside={`declares ${declares(glyph)}`}
            flush
        >
            {SPACING_FIELDS.map((key) => {
                const src = glyph.fields[key];
                const value = sp[key];
                return (
                    <div
                        key={key}
                        className={`${sheet.row} ${styles.fieldRow}`}
                        data-new={(src && !initial.has(key)) || undefined}
                    >
                        <span style={{ fontWeight: 500 }}>{key}</span>
                        <span className={styles.cell}>
                            <ExprField
                                key={src ?? ""}
                                className={styles.input}
                                label={key}
                                value={src ?? ""}
                                placeholder={`derived · ${fmt(value)}`}
                                onCommit={(text) =>
                                    typeSpacing(glyph.name, key, text)
                                }
                            />
                        </span>
                        <span className={sheet.num}>{fmt(value)}</span>
                    </div>
                );
            })}
            <p className={styles.explain}>
                Drag a guide to change its bearing, or the letterform to shift
                it within its advance. A typed number adds to a declared field;
                a derived one moves its guide, declaring a field when none can
                take the change.
            </p>
        </Section>
    );
}

function MetricLines({ font }: { font: FontData }) {
    const select = useStore((s) => s.select);
    return (
        <Section title="Metric lines" aside="y · overshoot" flush>
            {font.metrics.map((m) => (
                <div
                    key={m.name}
                    className={`${sheet.row} ${styles.metricRow}`}
                >
                    <button
                        type="button"
                        className={styles.metricName}
                        onClick={() =>
                            select({
                                kind: "metric",
                                name: m.name,
                                span: m.span,
                                origin: "canvas",
                            })
                        }
                    >
                        {m.name}
                    </button>
                    <span className={styles.cell}>
                        <ExprField
                            key={m.expr}
                            className={styles.input}
                            label={`${m.name} y`}
                            value={m.expr}
                            onCommit={(text) => typeMetric(m.name, "y", text)}
                        />
                    </span>
                    <span className={sheet.num}>{fmt(m.y)}</span>
                    <span className={styles.cell}>
                        <ExprField
                            key={m.overshootExpr ?? ""}
                            className={styles.input}
                            label={`${m.name} overshoot`}
                            value={m.overshootExpr ?? ""}
                            placeholder="0"
                            onCommit={(text) =>
                                typeMetric(m.name, "overshoot", text)
                            }
                        />
                    </span>
                </div>
            ))}
        </Section>
    );
}

function FontInfoForm() {
    const info = useStore((s) => s.lastGood?.font);
    return (
        <Section title="Font info" flush>
            {FONT_FIELDS.map(({ key, label }) => {
                const raw = info?.[key];
                const value = raw === undefined ? "" : String(raw);
                return (
                    <div key={key} className={`${sheet.row} ${styles.infoRow}`}>
                        <span className={sheet.key}>{label}</span>
                        <span className={styles.cell}>
                            <ExprField
                                key={value}
                                className={`${styles.input} ${key === "em" ? "" : styles.str}`}
                                label={label}
                                value={value}
                                placeholder="—"
                                onCommit={(text) => setFontField(key, text)}
                            />
                        </span>
                    </div>
                );
            })}
        </Section>
    );
}
