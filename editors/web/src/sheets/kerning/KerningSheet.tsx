import { type PointerEvent, useEffect, useRef, useState } from "react";
import type {
    FontData,
    GlyphInfo,
    GroupInfo,
    KernInfo,
} from "../../engine/types.ts";
import {
    type CoveredPair,
    effectiveKern,
    fmt,
    glyphsForText,
    kernLevel,
    kernPairs,
    kernSample,
    verticalExtent,
} from "../../font/lookup.ts";
import {
    deleteGroup,
    deleteKern,
    groupMember,
    KernDrag,
    nudgeKern,
    setKernUnit,
} from "../../state/kerning.ts";
import { isTyping } from "../../state/shortcuts.ts";
import { useStore } from "../../state/store.ts";
import {
    Ball,
    Centre,
    Empty,
    LeftColumn,
    Section,
    sheet,
} from "../../ui/Sheet.tsx";
import { MemberPicker, PairPicker } from "./GlyphPicker.tsx";
import styles from "./KerningSheet.module.css";

function side(name: string | undefined, group: boolean) {
    return group ? `@${name}` : (name ?? "?");
}

export function KerningSheet() {
    const font = useStore((s) => s.font);
    const kernIndex = useStore((s) => s.kern);
    const setKern = useStore((s) => s.setKern);
    const select = useStore((s) => s.select);
    const context = useStore((s) => s.kernContext);
    const setContext = useStore((s) => s.setKernContext);
    const [adding, setAdding] = useState(false);
    /** The covered pair picked to show a group kern with. */
    const [sample, setSample] = useState<{
        index: number;
        pair: [string, string];
    } | null>(null);

    const index = font
        ? (kernIndex ?? (font.kerns.length > 0 ? 0 : null))
        : null;

    // ←/→ nudge the active pair; ⇧ nudges by 10 (plan 6, §4).
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (isTyping(e) || e.ctrlKey || e.metaKey || e.altKey) return;
            if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
            if (index === null) return;
            e.preventDefault();
            const step = e.shiftKey ? 10 : 1;
            nudgeKern(index, e.key === "ArrowLeft" ? -step : step);
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [index]);

    const toolbar = (
        <>
            <span className={sheet.view}>View 04 · Kerning</span>
            <span className={sheet.toolLabel}>Context</span>
            <input
                className={sheet.toolInput}
                style={{ minWidth: "14rem" }}
                value={context}
                spellCheck={false}
                title="Lines separated by ' · '; <pair> is replaced by the selected pair"
                onChange={(e) => setContext(e.target.value)}
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

    const kern = index === null ? undefined : font.kerns[index];
    const covered = index === null ? [] : kernPairs(font, index);
    const picked =
        sample &&
        sample.index === index &&
        covered.some(
            (c) => c.left === sample.pair[0] && c.right === sample.pair[1],
        )
            ? sample.pair
            : undefined;
    const [left, right] =
        picked ?? (index === null ? undefined : kernSample(font, index)) ?? [];
    const pick = (i: number) => {
        setKern(i);
        select({
            kind: "kern",
            name: `${i}`,
            span: font.kerns[i].span,
            origin: "canvas",
        });
    };

    return (
        <>
            <LeftColumn>
                <Groups font={font} />
                {left && <Effective font={font} left={left} />}
            </LeftColumn>
            <Centre toolbar={toolbar}>
                {kern && index !== null && left && right ? (
                    <>
                        <div className={styles.lines}>
                            {contextLines(font, context, left, right).map(
                                (seq, i) => (
                                    <KernLine
                                        // biome-ignore lint/suspicious/noArrayIndexKey: lines are positional
                                        key={i}
                                        font={font}
                                        seq={seq}
                                        left={left}
                                        right={right}
                                        index={index}
                                    />
                                ),
                            )}
                        </div>
                        {(kern.leftGroup || kern.rightGroup) && (
                            <CoveredPairs
                                font={font}
                                index={index}
                                pairs={covered}
                                shown={[left, right]}
                                onShow={(pair) => setSample({ index, pair })}
                            />
                        )}
                        <NudgeBar
                            kern={kern}
                            index={index}
                            left={left}
                            right={right}
                        />
                    </>
                ) : (
                    <Empty>
                        This font has no kern pairs yet: add one with + New pair
                        below.
                    </Empty>
                )}
                <div className={styles.bomWrap}>
                    <div className={sheet.bomHead}>
                        <span className={sheet.bomTitle}>
                            Bill of materials — kern pairs
                        </span>
                        <span className={styles.bomActions}>
                            <span className={sheet.aside}>
                                {font.kerns.length} ITEMS
                            </span>
                            <button
                                type="button"
                                className={styles.newButton}
                                onClick={() => setAdding(true)}
                            >
                                + New pair
                            </button>
                        </span>
                    </div>
                    {adding && (
                        <PairPicker
                            font={font}
                            onClose={() => setAdding(false)}
                        />
                    )}
                    <div className={sheet.bom}>
                        <div
                            className={`${sheet.bomRow} ${sheet.bomHeader} ${styles.headCols}`}
                        >
                            <span>Item</span>
                            <span>Left</span>
                            <span>Right</span>
                            <span>By</span>
                            <span>Unit</span>
                            <span>Level</span>
                            <span />
                        </div>
                        {font.kerns.map((k, i) => {
                            const on = i === index;
                            const name = `${side(k.left, k.leftGroup)} → ${side(k.right, k.rightGroup)}`;
                            return (
                                <div
                                    // biome-ignore lint/suspicious/noArrayIndexKey: kerns have no names
                                    key={i}
                                    className={styles.bomLine}
                                    data-on={on || undefined}
                                >
                                    <button
                                        type="button"
                                        className={`${sheet.bomRow} ${styles.cols} ${styles.bomPick}`}
                                        onClick={() => pick(i)}
                                    >
                                        <span>
                                            <Ball n={i + 1} on={on} />
                                        </span>
                                        <span style={{ fontWeight: 500 }}>
                                            {side(k.left, k.leftGroup)}
                                        </span>
                                        <span>
                                            {side(k.right, k.rightGroup)}
                                        </span>
                                        <span
                                            className={sheet.num}
                                            title={k.expr}
                                            style={{
                                                justifyContent: "flex-end",
                                                color: on
                                                    ? "var(--acc)"
                                                    : undefined,
                                            }}
                                        >
                                            {fmt(k.by)}
                                        </span>
                                        <span style={{ color: "var(--mid)" }}>
                                            {unit(k)}
                                        </span>
                                        <span style={{ color: "var(--mid)" }}>
                                            {kernLevel(k)}
                                        </span>
                                    </button>
                                    <button
                                        type="button"
                                        className={styles.rowDelete}
                                        aria-label={`Delete pair ${name}`}
                                        title={`Delete ${name}`}
                                        onClick={() => void deleteKern(i)}
                                    >
                                        ×
                                    </button>
                                </div>
                            );
                        })}
                    </div>
                </div>
            </Centre>
        </>
    );
}

function unit(kern: KernInfo) {
    return /\d(?:\.\d+)?em\s*$/.test(kern.expr) ? "em" : "raw";
}

type Seq = { glyph: GlyphInfo; pair: boolean }[];

/** A context template's glyphs: the text around `<pair>`, the pair in
 * place of it; `context` counts the glyphs that aren't the pair. */
function lineGlyphs(
    font: FontData,
    template: string,
    left: string,
    right: string,
): { seq: Seq; context: number } {
    const byName = new Map(font.glyphs.map((g) => [g.name, g]));
    const pairL = byName.get(left);
    const pairR = byName.get(right);
    const [before, after = ""] = template.split("<pair>");
    const around = (text: string) =>
        glyphsForText(font, text).map((glyph) => ({ glyph, pair: false }));
    const pre = around(before);
    const post = around(after);
    const pair =
        pairL && pairR && template.includes("<pair>")
            ? [
                  { glyph: pairL, pair: true },
                  { glyph: pairR, pair: true },
              ]
            : [];
    return {
        seq: [...pre, ...pair, ...post],
        context: pre.length + post.length,
    };
}

/**
 * The context lines to draw: one per ` · `-separated template. A line
 * whose context glyphs the font lacks would show only the bare pair, so
 * it is dropped (unless every line is), as is a repeat of another line.
 */
function contextLines(
    font: FontData,
    context: string,
    left: string,
    right: string,
): Seq[] {
    const lines = context
        .split(" · ")
        .map((t) => lineGlyphs(font, t, left, right))
        .filter((l) => l.seq.length > 0);
    const withContext = lines.filter((l) => l.context > 0);
    const seen = new Set<string>();
    return (withContext.length > 0 ? withContext : lines.slice(0, 1))
        .map((l) => l.seq)
        .filter((seq) => {
            const key = seq.map((i) => i.glyph.name).join(" ");
            if (seen.has(key)) return false;
            seen.add(key);
            return true;
        });
}

/** One context line: glyphs set with kerning, the pair highlighted. The
 * pair's right glyph drags horizontally to change `by`. */
function KernLine({
    font,
    seq,
    left,
    right,
    index,
}: {
    font: FontData;
    seq: Seq;
    left: string;
    right: string;
    index: number;
}) {
    const svgRef = useRef<SVGSVGElement>(null);
    const dragRef = useRef<{ drag: KernDrag; start: number } | null>(null);
    /** The viewBox while dragging, so the scale holds still. */
    const [frozen, setFrozen] = useState<number[] | null>(null);

    const toSvgX = (e: { clientX: number; clientY: number }) => {
        const m = svgRef.current?.getScreenCTM();
        if (!m) return null;
        return new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse())
            .x;
    };

    if (seq.length === 0) return null;

    const [descender, ascender] = verticalExtent(font);
    const height = ascender - descender;
    const fs = height * 0.06;
    let x = 0;
    let guide: [number, number] | null = null;
    const placed = seq.map((item, i) => {
        const second = item.pair && i > 0 && seq[i - 1].pair;
        if (i > 0) {
            const k =
                effectiveKern(font, seq[i - 1].glyph.name, item.glyph.name)
                    ?.value ?? 0;
            if (second) guide = [x, x + k];
            x += k;
        }
        const at = x;
        x += item.glyph.advance ?? 0;
        return { ...item, at, second };
    });
    const total = x;
    const vb = frozen ?? [
        -height * 0.1,
        -ascender - fs * 3,
        total + height * 0.2,
        height + fs * 3.5,
    ];
    const g = guide as [number, number] | null;
    const [lo, hi] = g ? [Math.min(...g), Math.max(...g)] : [0, 0];

    const begin = (e: PointerEvent<SVGRectElement>) => {
        const at = toSvgX(e);
        if (e.button !== 0 || dragRef.current || at === null) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        dragRef.current = { drag: new KernDrag(index), start: at };
        setFrozen(vb);
    };
    const move = (e: PointerEvent) => {
        const d = dragRef.current;
        const at = d && toSvgX(e);
        if (!d || at === null || at === undefined) return;
        d.drag.move(Math.round(at - d.start));
    };
    const end = (cancel: boolean) => {
        const d = dragRef.current;
        if (!d) return;
        dragRef.current = null;
        void d.drag.end(cancel).then(() => setFrozen(null));
    };

    return (
        <svg
            ref={svgRef}
            className={styles.line}
            viewBox={vb.join(" ")}
            onPointerMove={move}
            onPointerUp={() => end(false)}
            onPointerCancel={() => end(true)}
        >
            <title>{seq.map((item) => item.glyph.name).join(" ")}</title>
            <line
                x1={vb[0]}
                x2={vb[0] + vb[2]}
                y1={0}
                y2={0}
                className={styles.baseline}
            />
            {g && (
                <>
                    <line
                        x1={g[0]}
                        x2={g[0]}
                        y1={-ascender}
                        y2={-descender}
                        className={styles.guide}
                    />
                    <line
                        x1={g[1]}
                        x2={g[1]}
                        y1={-ascender}
                        y2={-descender}
                        className={styles.guide}
                    />
                    <line
                        x1={lo}
                        x2={hi}
                        y1={-ascender - fs * 1.2}
                        y2={-ascender - fs * 1.2}
                        className={styles.dim}
                    />
                    <text
                        x={hi + fs * 0.4}
                        y={-ascender - fs * 0.9}
                        fontSize={fs * 1.2}
                        className={styles.dimText}
                    >
                        {fmt(g[1] - g[0])}
                    </text>
                </>
            )}
            {placed.map((item, i) => (
                <path
                    // biome-ignore lint/suspicious/noArrayIndexKey: a line can repeat glyphs
                    key={i}
                    d={item.glyph.outline}
                    transform={`translate(${item.at} 0) scale(1 -1)`}
                    className={item.pair ? styles.pair : styles.ink}
                />
            ))}
            {/* The pair's right glyph drags by its whole advance box, not
                just its ink. */}
            {placed
                .filter((item) => item.second)
                .map((item) => (
                    <rect
                        key="drag"
                        x={item.at}
                        y={-ascender}
                        width={item.glyph.advance ?? 0}
                        height={height}
                        className={styles.dragHit}
                        onPointerDown={begin}
                    >
                        <title>{`Drag to kern ${left} → ${right}`}</title>
                    </rect>
                ))}
        </svg>
    );
}

/** Every pair a group kern covers, each set with the kern it actually
 * gets: this one (accent), or a glyph pair that overrides it (dimmed,
 * with its value). Clicking one shows it in the context lines. */
function CoveredPairs({
    font,
    index,
    pairs,
    shown,
    onShow,
}: {
    font: FontData;
    index: number;
    pairs: CoveredPair[];
    shown: [string, string];
    onShow: (pair: [string, string]) => void;
}) {
    const byName = new Map(font.glyphs.map((g) => [g.name, g]));
    const [descender, ascender] = verticalExtent(font);
    const height = ascender - descender;
    const applying = pairs.filter((p) => p.effective?.index === index).length;
    return (
        <div className={styles.covered}>
            <div className={styles.coveredHead}>
                <span className="label">Covers {pairs.length} pairs</span>
                <span className={styles.hint}>
                    {applying} kerned by this pair
                    {applying < pairs.length
                        ? ` · ${pairs.length - applying} overridden by glyph pairs`
                        : ""}
                </span>
            </div>
            <div className={styles.coveredGrid}>
                {pairs.map(({ left, right, effective }) => {
                    const l = byName.get(left);
                    const r = byName.get(right);
                    if (!l || !r) return null;
                    const own = effective?.index === index;
                    const k = effective?.value ?? 0;
                    const x = (l.advance ?? 0) + k;
                    const width = x + (r.advance ?? 0);
                    const on = shown[0] === left && shown[1] === right;
                    const pad = height * 0.06;
                    return (
                        <button
                            type="button"
                            key={`${left} ${right}`}
                            className={styles.cell}
                            data-own={own || undefined}
                            data-on={on || undefined}
                            title={
                                own
                                    ? `${left} → ${right}: ${fmt(k)}`
                                    : `${left} → ${right}: ${fmt(k)}, from a glyph pair that overrides this one`
                            }
                            onClick={() => onShow([left, right])}
                        >
                            <svg
                                className={styles.cellArt}
                                viewBox={[
                                    -pad,
                                    -ascender - pad,
                                    width + 2 * pad,
                                    height + 2 * pad,
                                ].join(" ")}
                                aria-hidden
                            >
                                <path d={l.outline} transform="scale(1 -1)" />
                                <path
                                    d={r.outline}
                                    transform={`translate(${x} 0) scale(1 -1)`}
                                />
                            </svg>
                            <span className={styles.cellMeta}>
                                <span>
                                    {left} {right}
                                </span>
                                <span>{own ? fmt(k) : `${fmt(k)} ⤴`}</span>
                            </span>
                        </button>
                    );
                })}
            </div>
        </div>
    );
}

function NudgeBar({
    kern,
    index,
    left,
    right,
}: {
    kern: KernInfo;
    index: number;
    left: string;
    right: string;
}) {
    const u = unit(kern);
    return (
        <div className={styles.nudge}>
            {[-10, -1].map((d) => (
                <button
                    type="button"
                    key={d}
                    title={d === -1 ? "←" : "⇧←"}
                    onClick={() => nudgeKern(index, d)}
                >
                    {`−${-d}`}
                </button>
            ))}
            <div className={styles.value}>
                <span className={styles.pairName}>
                    {left} → {right}
                </span>
                <span className={styles.by} title={kern.expr}>
                    {fmt(kern.by)}
                </span>
                <span className={styles.hint}>← → nudge · ⇧ ×10</span>
            </div>
            {[1, 10].map((d) => (
                <button
                    type="button"
                    key={d}
                    title={d === 1 ? "→" : "⇧→"}
                    onClick={() => nudgeKern(index, d)}
                >
                    {`+${d}`}
                </button>
            ))}
            {(["raw", "em"] as const).map((to) => (
                <button
                    type="button"
                    key={to}
                    className={styles.unit}
                    data-on={u === to || undefined}
                    aria-pressed={u === to}
                    title={`Write ${kern.expr} in ${to === "em" ? "em" : "raw"} units`}
                    onClick={() => {
                        if (u !== to) void setKernUnit(index, to === "em");
                    }}
                >
                    {to}
                </button>
            ))}
        </div>
    );
}

// ---------------------------------------------------------------------
// Groups

/** The font's groups. New ones come from picking several glyphs for a
 * side of a new pair. */
function Groups({ font }: { font: FontData }) {
    return (
        <Section title="Groups" flush>
            {font.groups.length === 0 && (
                <p
                    className="note"
                    style={{ margin: 0, padding: "0.5rem 0.75rem" }}
                >
                    No groups yet: ⌘/Ctrl-click several glyphs for one side of a
                    new pair.
                </p>
            )}
            {font.groups.map((group) => (
                <Group key={group.name} font={font} group={group} />
            ))}
        </Section>
    );
}

function Group({ font, group }: { font: FontData; group: GroupInfo }) {
    const select = useStore((s) => s.select);
    const [adding, setAdding] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const asLeft = font.kerns.filter(
        (k) => k.leftGroup && k.left === group.name,
    ).length;
    const asRight = font.kerns.filter(
        (k) => k.rightGroup && k.right === group.name,
    ).length;
    const use = [asLeft && "left-used", asRight && "right-used"]
        .filter(Boolean)
        .join(" · ");
    const pairs = asLeft + asRight;
    const remove = async (glyph: string) => {
        const result = await groupMember(group.name, [glyph], false);
        setError(result?.status === "invalid" ? result.message : null);
    };
    /** A used group asks once: its pairs go with it. */
    const [confirming, setConfirming] = useState(false);
    const onDelete = () => {
        if (pairs > 0 && !confirming) {
            setConfirming(true);
            return;
        }
        void deleteGroup(group.name);
    };
    return (
        <div className={styles.group}>
            <div className={styles.groupHead}>
                <button
                    type="button"
                    className={styles.groupName}
                    onClick={() =>
                        select({
                            kind: "group",
                            name: group.name,
                            span: group.span,
                            origin: "canvas",
                        })
                    }
                >
                    {group.name}
                </button>
                <span className={sheet.key}>
                    {use || "unused"} · {pairs} pair{pairs === 1 ? "" : "s"}
                </span>
                <button
                    type="button"
                    className={styles.groupDelete}
                    data-confirm={confirming || undefined}
                    title={
                        pairs > 0
                            ? `Delete ${group.name} and the ${pairs} pair${pairs === 1 ? "" : "s"} using it`
                            : `Delete ${group.name}`
                    }
                    onClick={onDelete}
                    onBlur={() => setConfirming(false)}
                >
                    {confirming
                        ? `Delete + ${pairs} pair${pairs === 1 ? "" : "s"}?`
                        : "Delete"}
                </button>
            </div>
            <span className={styles.chips}>
                {group.glyphs.map((g) => (
                    <span key={g} className={styles.chip}>
                        {g}
                        <button
                            type="button"
                            aria-label={`Remove ${g} from ${group.name}`}
                            onClick={() => void remove(g)}
                        >
                            ×
                        </button>
                    </span>
                ))}
                <button
                    type="button"
                    className={styles.chipAdd}
                    aria-label={`Add glyphs to ${group.name}`}
                    onClick={() => setAdding(true)}
                >
                    +
                </button>
            </span>
            {error && <p className={styles.error}>{error}</p>}
            {adding && (
                <MemberPicker
                    font={font}
                    group={group}
                    onClose={() => setAdding(false)}
                />
            )}
        </div>
    );
}

function Effective({ font, left }: { font: FontData; left: string }) {
    const rows = font.glyphs
        .map((g) => ({ g, eff: effectiveKern(font, left, g.name) }))
        .filter((r) => r.eff);
    return (
        <Section title={`Effective · ${left} → right`} flush>
            {rows.map(({ g, eff }) => {
                if (!eff) return null;
                const k = font.kerns[eff.index];
                const why =
                    eff.level === "glyph"
                        ? "glyph pair · wins"
                        : `group ${k.rightGroup ? k.right : k.left}`;
                return (
                    <div
                        key={g.name}
                        className={`${sheet.row} ${styles.effRow}`}
                    >
                        <span style={{ fontWeight: 500 }}>{g.name}</span>
                        <span
                            className={sheet.num}
                            style={{
                                color:
                                    eff.level === "glyph"
                                        ? "var(--acc)"
                                        : undefined,
                            }}
                        >
                            {fmt(eff.value)}
                        </span>
                        <span style={{ color: "var(--mid)" }}>{why}</span>
                    </div>
                );
            })}
            <p
                className="note"
                style={{ margin: 0, padding: "0.5rem 0.75rem" }}
            >
                A glyph pair overrides a group pair.
            </p>
        </Section>
    );
}
