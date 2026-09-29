import { type ReactNode, useRef } from "react";
import type {
    ArcInfo,
    FontData,
    GlyphInfo,
    GlyphScene,
    PathInfo,
    Pt,
} from "../../engine/types.ts";
import { fmt, pathKey, verticalExtent } from "../../font/lookup.ts";
import type { Selection } from "../../state/store.ts";
import { useStore } from "../../state/store.ts";
import { type Box, useViewport, viewBox } from "../../svg/viewport.ts";
import styles from "./GlyphCanvas.module.css";

// Room kept clear of the overlays: metric labels on the left, the title
// block at the top right, the tool palette at the bottom.
const INSETS = { top: 56, right: 48, bottom: 88, left: 150 };

/** Radius labels on `polar` rays and arcs; hidden until they get better
 * placement. */
const SHOW_RADII = false;

/** Font y → SVG y. */
const Y = (y: number) => -y;

/** `xHeight` → `X HEIGHT`. */
function metricLabel(name: string) {
    return name.replace(/([a-z])([A-Z])/g, "$1 $2").toUpperCase();
}

function glyphBox(font: FontData, glyph: GlyphInfo): Box {
    const [descender, ascender] = verticalExtent(font);
    const shift = glyph.shift ?? 0;
    const advance = glyph.advance ?? font.em / 2;
    let x0 = -shift;
    let x1 = advance - shift;
    if (glyph.ink) {
        x0 = Math.min(x0, glyph.ink[0]);
        x1 = Math.max(x1, glyph.ink[2]);
    }
    return [x0, descender, x1, ascender];
}

export function GlyphCanvas({
    font,
    glyph,
    scene,
}: {
    font: FontData;
    glyph: GlyphInfo;
    scene: GlyphScene;
}) {
    const layers = useStore((s) => s.layers);
    const selection = useStore((s) => s.selection);
    const select = useStore((s) => s.select);
    const setGlyph = useStore((s) => s.setGlyph);
    const setPointer = useStore((s) => s.setPointer);
    const vp = useViewport(glyphBox(font, glyph), INSETS);
    const pan = useRef<{ x: number; y: number } | null>(null);

    const { w, h } = vp.size;
    const vb = viewBox(vp.view, w, h);
    const k = 1 / vp.view.scale; // font units per pixel
    const [descender, ascender] = verticalExtent(font);
    const shift = glyph.shift ?? 0;
    const originX = -shift;
    const advanceX = (glyph.advance ?? 0) - shift;
    const left = vb[0];
    const right = vb[0] + vb[2];
    const top = vb[1];
    const bottom = vb[1] + vb[3];

    const pick =
        (sel: Omit<Selection, "origin">) => (e: React.PointerEvent) => {
            if (e.button !== 0) return;
            e.stopPropagation();
            select({ ...sel, origin: "canvas" });
        };
    const isSelected = (kind: Selection["kind"], name: string) =>
        selection?.kind === kind && selection.name === name;

    const text = (
        x: number,
        y: number,
        body: string,
        opts: {
            size?: number;
            cls?: string;
            anchor?: "start" | "middle" | "end";
        } = {},
    ) => (
        <text
            x={x}
            y={y}
            className={opts.cls ?? styles.mono}
            fontSize={(opts.size ?? 11) * k}
            textAnchor={opts.anchor ?? "start"}
        >
            {body}
        </text>
    );

    // ── grid ────────────────────────────────────────────────────────────
    const step = gridStep(k);
    const grid: ReactNode[] = [];
    for (let x = Math.ceil(left / step) * step; x <= right; x += step) {
        grid.push(<line key={`x${x}`} x1={x} y1={top} x2={x} y2={bottom} />);
    }
    for (let y = Math.ceil(top / step) * step; y <= bottom; y += step) {
        grid.push(<line key={`y${y}`} x1={left} y1={y} x2={right} y2={y} />);
    }

    // ── metrics ─────────────────────────────────────────────────────────
    const metrics = font.metrics
        .filter((m) => m.y !== undefined)
        .map((m) => {
            const y = Y(m.y as number);
            const base = m.name === "baseline";
            return (
                <g key={m.name}>
                    <line
                        x1={left}
                        y1={y}
                        x2={right}
                        y2={y}
                        className={base ? styles.baseline : styles.metric}
                    />
                    {text(
                        left + 8 * k,
                        y - 5 * k,
                        `${metricLabel(m.name)} ${fmt(m.y)}`,
                        {
                            cls: styles.metricLabel,
                            size: 10,
                        },
                    )}
                    {m.expr &&
                        m.expr !== fmt(m.y) &&
                        text(left + 8 * k, y + 13 * k, m.expr, {
                            cls: styles.faint,
                            size: 9.5,
                        })}
                </g>
            );
        });

    // ── guides ──────────────────────────────────────────────────────────
    const guideLabelY = Y(descender) + 30 * k;
    const guides = (
        <g>
            <line
                x1={originX}
                y1={top}
                x2={originX}
                y2={bottom}
                className={styles.guide}
            />
            <line
                x1={advanceX}
                y1={top}
                x2={advanceX}
                y2={bottom}
                className={styles.guide}
            />
            {text(originX + 6 * k, guideLabelY, "origin 0", { size: 10.5 })}
            {text(
                advanceX - 6 * k,
                guideLabelY,
                `advance ${glyph.fields.advance ? `${glyph.fields.advance} = ` : ""}${fmt(glyph.advance)}`,
                { size: 10.5, anchor: "end" },
            )}
        </g>
    );

    // ── outline and components ──────────────────────────────────────────
    const outline = (
        <g transform="scale(1 -1)">
            <path d={scene.outline.join(" ")} className={styles.outline} />
            {scene.components.map((c, i) => (
                // biome-ignore lint/a11y/noStaticElementInteractions: canvas shapes; the source pane is the keyboard route
                <path
                    // biome-ignore lint/suspicious/noArrayIndexKey: components have no names
                    key={i}
                    d={c.outline}
                    className={styles.component}
                    data-on={isSelected("component", `${i}`) || undefined}
                    onPointerDown={pick({
                        kind: "component",
                        name: `${i}`,
                        span: c.span,
                    })}
                    onDoubleClick={() => setGlyph(c.glyph)}
                >
                    <title>{`component ${c.glyph} · double-click to open`}</title>
                </path>
            ))}
        </g>
    );

    // ── construction ────────────────────────────────────────────────────
    const construction = (
        <g>
            {scene.lines.map((l) => {
                // A `polar` point's ray is drawn from its centre to the
                // point only, unlabelled (the point carries the name), and
                // selects and highlights with the point. Lines are
                // infinite and labelled at the right edge.
                const [a, b] = l.of ? [l.p0, l.p1] : extend(l.p0, l.p1, 1e5);
                const label = l.of
                    ? null
                    : labelOnLine(l.p0, l.p1, right - 10 * k, top, bottom);
                const kind = l.of ? "point" : "line";
                return (
                    <g
                        key={`${kind}:${l.name}`}
                        className={styles.conLine}
                        data-on={isSelected(kind, l.name) || undefined}
                    >
                        <path
                            d={`M${a[0]} ${Y(a[1])} L${b[0]} ${Y(b[1])}`}
                            className={styles.hit}
                            onPointerDown={pick({
                                kind,
                                name: l.name,
                                span: l.span,
                            })}
                        />
                        <line x1={a[0]} y1={Y(a[1])} x2={b[0]} y2={Y(b[1])} />
                        {SHOW_RADII && l.of && (
                            <AlongLabel
                                a={l.p0}
                                b={l.p1}
                                k={k}
                                label={radiusLabel(
                                    Math.hypot(
                                        l.p1[0] - l.p0[0],
                                        l.p1[1] - l.p0[1],
                                    ),
                                    l.radiusExpr,
                                )}
                            />
                        )}
                        {label &&
                            text(
                                label[0],
                                label[1] - 6 * k,
                                `${l.name} · ${l.expr}`,
                                {
                                    cls: styles.conLabel,
                                    anchor: "end",
                                },
                            )}
                    </g>
                );
            })}
            {scene.paths.flatMap((path) =>
                path.segments.map((seg) =>
                    seg.arc && seg.to ? (
                        <ArcRadii
                            key={`arc@${seg.span[0]}`}
                            arc={seg.arc}
                            to={seg.to}
                            scene={scene}
                            k={k}
                        />
                    ) : null,
                ),
            )}
            {scene.points
                .filter((p) => p.role === "construction")
                .map((p) => {
                    const s = (isSelected("point", p.name) ? 8 : 6.5) * k;
                    const [x, y] = [p.at[0], Y(p.at[1])];
                    return (
                        <g
                            key={p.name}
                            className={styles.conPoint}
                            data-on={isSelected("point", p.name) || undefined}
                        >
                            <path
                                d={`M${x} ${y - s} L${x + s} ${y} L${x} ${y + s} L${x - s} ${y} Z`}
                                onPointerDown={pick({
                                    kind: "point",
                                    name: p.name,
                                    span: p.span,
                                })}
                            />
                            {text(
                                x + 10 * k,
                                y + 16 * k,
                                p.callee ? `${p.name} · ${p.callee}` : p.name,
                                {
                                    cls: styles.conLabel,
                                },
                            )}
                        </g>
                    );
                })}
        </g>
    );

    // ── dimensions ──────────────────────────────────────────────────────
    const dims: ReactNode[] = [];
    if (glyph.advance !== undefined) {
        dims.push(
            <Dim
                key="advance"
                a={[originX, 0]}
                b={[advanceX, 0]}
                at={Y(descender) + 58 * k}
                k={k}
                label={`${glyph.fields.advance ? `${glyph.fields.advance} = ` : ""}${fmt(glyph.advance)}`}
            />,
        );
    }
    const cap = font.metrics.find((m) => m.name === "capHeight");
    if (cap?.y !== undefined) {
        dims.push(
            <VDim
                key="cap"
                y0={0}
                y1={cap.y}
                at={Math.min(originX, glyph.ink?.[0] ?? originX) - 44 * k}
                k={k}
                label={
                    cap.expr && cap.expr !== fmt(cap.y)
                        ? `${cap.expr} = ${fmt(cap.y)}`
                        : fmt(cap.y)
                }
            />,
        );
    }
    const selPoint =
        selection?.kind === "point"
            ? scene.points.find((p) => p.name === selection.name)
            : undefined;
    if (selPoint) {
        dims.push(
            <Dim
                key="sel"
                a={[0, selPoint.at[1]]}
                b={[selPoint.at[0], selPoint.at[1]]}
                at={Y(ascender) - 18 * k}
                k={k}
                accent
                label={`${selPoint.name}.x = ${fmt(selPoint.at[0])}`}
            />,
        );
    }
    for (const path of scene.paths) {
        // On the skeleton itself, halfway along the first segment.
        const anchor = path.anchor;
        if (!path.name || !anchor) continue;
        const [x, y] = [anchor[0], Y(anchor[1])];
        const label = `PATH ${path.name}`;
        const detail = path.stroke
            ? `STROKE ⌀ ${path.stroke}${path.caps ? ` · ${path.caps[0].toUpperCase()} CAPS` : ""}`
            : path.fill
              ? "FILL"
              : "CONSTRUCTION";
        dims.push(
            <g key={`callout-${path.index}`} className={styles.callout}>
                <line
                    x1={x}
                    y1={y}
                    x2={x + 36 * k}
                    y2={y - 36 * k}
                    className={styles.calloutLine}
                />
                <line
                    x1={x + 36 * k}
                    y1={y - 36 * k}
                    x2={x + 48 * k}
                    y2={y - 36 * k}
                    className={styles.calloutLine}
                />
                {text(x + 52 * k, y - 38 * k, label, {
                    cls: styles.calloutTitle,
                    size: 10,
                })}
                {text(x + 52 * k, y - 26 * k, detail, {
                    cls: styles.faint,
                    size: 9,
                })}
            </g>,
        );
    }

    // ── skeleton ────────────────────────────────────────────────────────
    const labelled = new Set<string>();
    const skeleton = (
        <g>
            <g transform="scale(1 -1)">
                {scene.paths.map((path) =>
                    path.skeleton ? (
                        <path
                            key={path.index}
                            d={path.skeleton}
                            className={styles.skeleton}
                            data-construction={
                                !(path.enabled && (path.stroke || path.fill)) ||
                                undefined
                            }
                            data-on={
                                isSelected("path", pathKey(path)) || undefined
                            }
                            onPointerDown={pick({
                                kind: "path",
                                name: pathKey(path),
                                span: path.span,
                            })}
                        />
                    ) : null,
                )}
            </g>
            {scene.paths.flatMap((path) =>
                handles(path).map(({ seg, i, prev }) => {
                    if (!seg.to) return null;
                    const [x, y] = [seg.to[0], Y(seg.to[1])];
                    const point = seg.toRef
                        ? scene.points.find((p) => p.name === seg.toRef)
                        : undefined;
                    const sel = point
                        ? isSelected("point", point.name)
                        : isSelected("segment", `${pathKey(path)}/${i}`);
                    const s = (sel ? 10 : 8) * k;
                    const showLabel = seg.toRef && !labelled.has(seg.toRef);
                    if (seg.toRef) labelled.add(seg.toRef);
                    return (
                        <g key={`${path.index}/${i}`}>
                            {seg.controls.map((c, ci) => {
                                const anchor =
                                    seg.kind === "cube" && ci === 0
                                        ? prev
                                        : seg.to;
                                if (!anchor) return null;
                                return (
                                    // biome-ignore lint/suspicious/noArrayIndexKey: controls are positional
                                    <g key={ci} className={styles.control}>
                                        <line
                                            x1={anchor[0]}
                                            y1={Y(anchor[1])}
                                            x2={c[0]}
                                            y2={Y(c[1])}
                                            className={styles.controlLine}
                                        />
                                        {seg.kind === "quad" && prev && (
                                            <line
                                                x1={prev[0]}
                                                y1={Y(prev[1])}
                                                x2={c[0]}
                                                y2={Y(c[1])}
                                                className={styles.controlLine}
                                            />
                                        )}
                                        <circle
                                            cx={c[0]}
                                            cy={Y(c[1])}
                                            r={3.5 * k}
                                        />
                                    </g>
                                );
                            })}
                            <rect
                                x={x - s / 2}
                                y={y - s / 2}
                                width={s}
                                height={s}
                                className={styles.handle}
                                data-on={sel || undefined}
                                onPointerDown={
                                    point
                                        ? pick({
                                              kind: "point",
                                              name: point.name,
                                              span: point.span,
                                          })
                                        : pick({
                                              kind: "segment",
                                              name: `${pathKey(path)}/${i}`,
                                              span: seg.span,
                                          })
                                }
                            />
                            {showLabel &&
                                text(
                                    x + 10 * k,
                                    y - 10 * k,
                                    seg.toRef as string,
                                    {
                                        cls: sel
                                            ? styles.handleLabelOn
                                            : styles.handleLabel,
                                    },
                                )}
                        </g>
                    );
                }),
            )}
        </g>
    );

    return (
        <svg
            ref={vp.ref}
            className={styles.canvas}
            viewBox={w > 0 ? vb.join(" ") : undefined}
            onPointerDown={(e) => {
                if (e.button === 1 || (e.button === 0 && e.altKey)) {
                    pan.current = { x: e.clientX, y: e.clientY };
                    e.currentTarget.setPointerCapture(e.pointerId);
                    e.preventDefault();
                } else if (e.button === 0) {
                    select(null);
                }
            }}
            onPointerMove={(e) => {
                setPointer(vp.toFont(e.clientX, e.clientY));
                if (pan.current) {
                    vp.panBy(
                        e.clientX - pan.current.x,
                        e.clientY - pan.current.y,
                    );
                    pan.current = { x: e.clientX, y: e.clientY };
                }
            }}
            onPointerUp={() => {
                pan.current = null;
            }}
            onPointerLeave={() => setPointer(null)}
            onWheel={(e) =>
                vp.zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * 0.0015))
            }
            onDoubleClick={(e) => {
                if (e.target === e.currentTarget) vp.reset();
            }}
        >
            <title>{`Glyph ${glyph.name}`}</title>
            {w > 0 && (
                <>
                    <g className={styles.grid}>{grid}</g>
                    {layers.metrics && metrics}
                    {layers.guides && guides}
                    {layers.outline && outline}
                    {layers.construction && construction}
                    {layers.dims && dims}
                    {layers.skeleton && skeleton}
                </>
            )}
        </svg>
    );
}

/** Each segment with the previous segment's end, for control handles. */
function handles(path: PathInfo) {
    return path.segments.map((seg, i) => ({
        seg,
        i,
        prev: i > 0 ? path.segments[i - 1].to : undefined,
    }));
}

function same(a: Pt, b: Pt) {
    return Math.abs(a[0] - b[0]) < 1e-6 && Math.abs(a[1] - b[1]) < 1e-6;
}

/** `r = 266.5`, or `r = arc_radius = 266.5` when an expression gave it. */
function radiusLabel(value: number, expr?: string, name = "r") {
    return expr && expr !== fmt(value)
        ? `${name} = ${expr} = ${fmt(value)}`
        : `${name} = ${fmt(value)}`;
}

/** An arc's centre and radius: spokes from the centre to both ends and the
 * radius, skipping any spoke a `polar` ray already draws (with its own
 * label), and a centre mark unless a point sits there. */
function ArcRadii({
    arc,
    to,
    scene,
    k,
}: {
    arc: ArcInfo;
    to: Pt;
    scene: GlyphScene;
    k: number;
}) {
    const ray = (end: Pt) =>
        scene.lines.some(
            (l) => l.of && same(l.p0, arc.center) && same(l.p1, end),
        );
    const spokes = [arc.from, to].filter((end) => !ray(end));
    const circle = Math.abs(arc.rx - arc.ry) < 1e-6;
    const label = circle
        ? radiusLabel(arc.rx, arc.rxExpr)
        : `${radiusLabel(arc.rx, arc.rxExpr, "rx")} · ${radiusLabel(arc.ry, arc.ryExpr, "ry")}`;
    const [cx, cy] = [arc.center[0], Y(arc.center[1])];
    const marked = scene.points.some((p) => same(p.at, arc.center));
    const m = 5 * k;
    return (
        <g className={styles.conLine}>
            {spokes.map((end, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: at most two spokes
                <line key={i} x1={cx} y1={cy} x2={end[0]} y2={Y(end[1])} />
            ))}
            {!marked && (
                <path
                    d={`M${cx - m} ${cy} L${cx + m} ${cy} M${cx} ${cy - m} L${cx} ${cy + m}`}
                    className={styles.centerMark}
                />
            )}
            {SHOW_RADII && spokes.length > 0 && (
                <AlongLabel
                    a={arc.center}
                    b={spokes[spokes.length - 1]}
                    k={k}
                    label={label}
                />
            )}
        </g>
    );
}

/** Text centred on the segment `a`–`b`, along it, just above it, never
 * upside down. */
function AlongLabel({
    a,
    b,
    k,
    label,
}: {
    a: Pt;
    b: Pt;
    k: number;
    label: string;
}) {
    const [x, y] = [(a[0] + b[0]) / 2, Y((a[1] + b[1]) / 2)];
    let angle = (Math.atan2(Y(b[1]) - Y(a[1]), b[0] - a[0]) * 180) / Math.PI;
    if (angle > 90) angle -= 180;
    if (angle < -90) angle += 180;
    return (
        <text
            x={x}
            y={y - 5 * k}
            fontSize={10.5 * k}
            textAnchor="middle"
            transform={`rotate(${angle} ${x} ${y})`}
            className={styles.conLabel}
        >
            {label}
        </text>
    );
}

function extend(p0: Pt, p1: Pt, length: number): [Pt, Pt] {
    const dx = p1[0] - p0[0];
    const dy = p1[1] - p0[1];
    const n = Math.hypot(dx, dy) || 1;
    return [
        [p0[0] - (dx / n) * length, p0[1] - (dy / n) * length],
        [p0[0] + (dx / n) * length, p0[1] + (dy / n) * length],
    ];
}

/** An SVG point on the line through `p0`/`p1` at SVG x `x`, if it lies
 * within `[top, bottom]`; for a vertical line, its top. */
function labelOnLine(
    p0: Pt,
    p1: Pt,
    x: number,
    top: number,
    bottom: number,
): Pt | null {
    const dx = p1[0] - p0[0];
    const dy = p1[1] - p0[1];
    if (Math.abs(dx) < 1e-9) return [p0[0] - 4, top + 40];
    const y = Y(p0[1] + ((x - p0[0]) * dy) / dx);
    return y > top && y < bottom ? [x, y] : null;
}

/** A grid step that is 40 px or more on screen: 10, 50, 100, 500, … */
function gridStep(unitsPerPx: number) {
    for (const step of [10, 50, 100, 500, 1000, 5000]) {
        if (step / unitsPerPx >= 40) return step;
    }
    return 10000;
}

function Dim({
    a,
    b,
    at,
    k,
    label,
    accent,
}: {
    a: Pt;
    b: Pt;
    at: number;
    k: number;
    label: string;
    accent?: boolean;
}) {
    const [x0, x1] = [a[0], b[0]];
    const mid = (x0 + x1) / 2;
    const width = (label.length * 6.6 + 12) * k;
    const t = 6 * k;
    return (
        <g className={styles.dim} data-accent={accent || undefined}>
            <line
                x1={x0}
                y1={Y(a[1])}
                x2={x0}
                y2={at + t}
                className={`${styles.dimLine} ${styles.ext}`}
            />
            <line
                x1={x1}
                y1={Y(b[1])}
                x2={x1}
                y2={at + t}
                className={`${styles.dimLine} ${styles.ext}`}
            />
            <line x1={x0} y1={at} x2={x1} y2={at} className={styles.dimLine} />
            <path
                d={`M${x0} ${at} l${10 * k} ${-3.5 * k} v${7 * k} z M${x1} ${at} l${-10 * k} ${-3.5 * k} v${7 * k} z`}
                className={styles.arrow}
            />
            <rect
                x={mid - width / 2}
                y={at - 8 * k}
                width={width}
                height={16 * k}
                className={styles.dimBox}
            />
            <text
                x={mid}
                y={at + 3.5 * k}
                fontSize={10.5 * k}
                textAnchor="middle"
                className={styles.dimText}
            >
                {label}
            </text>
        </g>
    );
}

function VDim({
    y0,
    y1,
    at,
    k,
    label,
}: {
    y0: number;
    y1: number;
    at: number;
    k: number;
    label: string;
}) {
    const [a, b] = [Y(y0), Y(y1)];
    const mid = (a + b) / 2;
    const width = (label.length * 6.6 + 12) * k;
    return (
        <g className={styles.dim}>
            <line x1={at} y1={a} x2={at} y2={b} className={styles.dimLine} />
            <path
                d={`M${at} ${a} l${-3.5 * k} ${-10 * k} h${7 * k} z M${at} ${b} l${-3.5 * k} ${10 * k} h${7 * k} z`}
                className={styles.arrow}
            />
            <rect
                x={at - 8 * k}
                y={mid - width / 2}
                width={16 * k}
                height={width}
                className={styles.dimBox}
            />
            <text
                x={at}
                y={mid}
                fontSize={10.5 * k}
                textAnchor="middle"
                dominantBaseline="central"
                transform={`rotate(-90 ${at} ${mid})`}
                className={styles.dimText}
            >
                {label}
            </text>
        </g>
    );
}
