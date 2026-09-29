import type { FontData, GlyphInfo, KernInfo } from "../../engine/types.ts";
import {
    effectiveKern,
    fmt,
    glyphsForText,
    kernLevel,
    sideGlyph,
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

    const index = kernIndex ?? (font.kerns.length > 0 ? 0 : null);
    const kern = index === null ? undefined : font.kerns[index];
    const left = kern && sideGlyph(font, kern.left, kern.leftGroup);
    const right = kern && sideGlyph(font, kern.right, kern.rightGroup);
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
                {kern && left && right ? (
                    <>
                        <div className={styles.lines}>
                            {context.split(" · ").map((line, i) => (
                                <KernLine
                                    // biome-ignore lint/suspicious/noArrayIndexKey: lines are positional
                                    key={i}
                                    font={font}
                                    template={line}
                                    left={left}
                                    right={right}
                                />
                            ))}
                        </div>
                        <NudgeBar kern={kern} left={left} right={right} />
                    </>
                ) : (
                    <Empty>
                        This font has no kern pairs yet. New pairs arrive in W7.
                    </Empty>
                )}
                <div className={styles.bomWrap}>
                    <div className={sheet.bomHead}>
                        <span className={sheet.bomTitle}>
                            Bill of materials — kern pairs
                        </span>
                        <span className={sheet.aside}>
                            {font.kerns.length} ITEMS
                        </span>
                    </div>
                    <div className={sheet.bom}>
                        <div
                            className={`${sheet.bomRow} ${sheet.bomHeader} ${styles.cols}`}
                        >
                            <span>Item</span>
                            <span>Left</span>
                            <span>Right</span>
                            <span>By</span>
                            <span>Unit</span>
                            <span>Level</span>
                        </div>
                        {font.kerns.map((k, i) => {
                            const on = i === index;
                            return (
                                <button
                                    type="button"
                                    // biome-ignore lint/suspicious/noArrayIndexKey: kerns have no names
                                    key={i}
                                    className={`${sheet.bomRow} ${styles.cols}`}
                                    data-on={on || undefined}
                                    onClick={() => pick(i)}
                                >
                                    <span>
                                        <Ball n={i + 1} on={on} />
                                    </span>
                                    <span style={{ fontWeight: 500 }}>
                                        {side(k.left, k.leftGroup)}
                                    </span>
                                    <span>{side(k.right, k.rightGroup)}</span>
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

/** One context line: glyphs set with kerning, the pair highlighted. */
function KernLine({
    font,
    template,
    left,
    right,
}: {
    font: FontData;
    template: string;
    left: string;
    right: string;
}) {
    const byName = new Map(font.glyphs.map((g) => [g.name, g]));
    const pairL = byName.get(left);
    const pairR = byName.get(right);
    const [before, after = ""] = template.split("<pair>");
    const seq: { glyph: GlyphInfo; pair: boolean }[] = [
        ...glyphsForText(font, before).map((glyph) => ({ glyph, pair: false })),
        ...(pairL && pairR && template.includes("<pair>")
            ? [
                  { glyph: pairL, pair: true },
                  { glyph: pairR, pair: true },
              ]
            : []),
        ...glyphsForText(font, after).map((glyph) => ({ glyph, pair: false })),
    ];
    if (seq.length === 0) return null;

    const [descender, ascender] = verticalExtent(font);
    const height = ascender - descender;
    const fs = height * 0.06;
    let x = 0;
    let guide: [number, number] | null = null;
    const placed = seq.map((item, i) => {
        if (i > 0) {
            const k =
                effectiveKern(font, seq[i - 1].glyph.name, item.glyph.name)
                    ?.value ?? 0;
            if (item.pair && seq[i - 1].pair) guide = [x, x + k];
            x += k;
        }
        const at = x;
        x += item.glyph.advance ?? 0;
        return { ...item, at };
    });
    const total = x;
    const vb = [
        -height * 0.1,
        -ascender - fs * 3,
        total + height * 0.2,
        height + fs * 3.5,
    ];
    const g = guide as [number, number] | null;
    const [lo, hi] = g ? [Math.min(...g), Math.max(...g)] : [0, 0];

    return (
        <svg
            className={styles.line}
            viewBox={vb.join(" ")}
            style={{ aspectRatio: `${vb[2]} / ${vb[3]}` }}
        >
            <title>{template.replace("<pair>", `${left}${right}`)}</title>
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
        </svg>
    );
}

function NudgeBar({
    kern,
    left,
    right,
}: {
    kern: KernInfo;
    left: string;
    right: string;
}) {
    const title = "Nudging arrives in W7";
    return (
        <div className={styles.nudge}>
            {["−10", "−1"].map((l) => (
                <button type="button" key={l} disabled title={title}>
                    {l}
                </button>
            ))}
            <div className={styles.value}>
                <span className="label">
                    {left} → {right}
                </span>
                <span className={styles.by}>{fmt(kern.by)}</span>
            </div>
            {["+1", "+10"].map((l) => (
                <button type="button" key={l} disabled title={title}>
                    {l}
                </button>
            ))}
            <span
                className={styles.unit}
                data-on={unit(kern) === "raw" || undefined}
            >
                Raw
            </span>
            <span
                className={styles.unit}
                data-on={unit(kern) === "em" || undefined}
            >
                Em
            </span>
        </div>
    );
}

function Groups({ font }: { font: FontData }) {
    const select = useStore((s) => s.select);
    return (
        <Section title="Groups" aside="+ new · W7" flush>
            {font.groups.length === 0 && (
                <p
                    className="note"
                    style={{ margin: 0, padding: "0.5rem 0.75rem" }}
                >
                    No groups.
                </p>
            )}
            {font.groups.map((group) => {
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
                return (
                    <button
                        type="button"
                        key={group.name}
                        className={styles.group}
                        onClick={() =>
                            select({
                                kind: "group",
                                name: group.name,
                                span: group.span,
                                origin: "canvas",
                            })
                        }
                    >
                        <span className={styles.groupHead}>
                            <span>{group.name}</span>
                            <span className={sheet.key}>
                                {use || "unused"} · {pairs} pair
                                {pairs === 1 ? "" : "s"}
                            </span>
                        </span>
                        <span className={styles.chips}>
                            {group.glyphs.map((g) => (
                                <span key={g}>{g}</span>
                            ))}
                        </span>
                    </button>
                );
            })}
        </Section>
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
