import { useEffect, useState } from "react";
import { drivers as fetchDrivers } from "../../engine/client.ts";
import type {
    FontData,
    GlyphInfo,
    GlyphScene,
    PathInfo,
    SegmentKind,
} from "../../engine/types.ts";
import { lockText } from "../../engine/types.ts";
import { fmt, hex, pathKey } from "../../font/lookup.ts";
import {
    removePathField,
    renameSelection,
    setFill,
    setPathField,
    setSegmentField,
    setSegmentKind,
} from "../../state/actions.ts";
import { dragging, endDrag, scrubDrag, startDrag } from "../../state/drag.ts";
import {
    disabledReason,
    RELATE_TOOLS,
    startRelate,
} from "../../state/relate.ts";
import type { Tool } from "../../state/store.ts";
import { LAYERS, useStore } from "../../state/store.ts";
import { Centre, Empty, LeftColumn, Section, sheet } from "../../ui/Sheet.tsx";
import { GlyphCanvas } from "./GlyphCanvas.tsx";
import styles from "./GlyphSheet.module.css";

const TOOLS: { key: Tool; glyph: string; name: string; hint: string }[] = [
    { key: "V", glyph: "↖", name: "Select / drag", hint: "click to select" },
    {
        key: "P",
        glyph: "✎",
        name: "Path",
        hint: "click: line · drag: curve · start: close · Esc ends",
    },
    { key: ".", glyph: "+", name: "Constr. point", hint: "click to place" },
    { key: "L", glyph: "/", name: "Line through", hint: "click two points" },
    {
        key: "G",
        glyph: "┼",
        name: "Guide",
        hint: "click: horizontal · ⇧: vertical",
    },
    { key: "M", glyph: "↔", name: "Measure", hint: "click two points" },
    {
        key: "C",
        glyph: "◫",
        name: "Component",
        hint: "pick a glyph, click to place",
    },
];

export function GlyphSheet() {
    const font = useStore((s) => s.font);
    const scene = useStore((s) => s.scene);
    const name = useStore((s) => s.glyph);
    const glyph = font?.glyphs.find((g) => g.name === name);

    return (
        <>
            <LeftColumn>
                {font && glyph && scene ? (
                    <Inspector glyph={glyph} scene={scene} />
                ) : null}
            </LeftColumn>
            <Centre toolbar={<Toolbar font={font} glyph={glyph} />}>
                {font && glyph && scene ? (
                    <div className={styles.stage}>
                        <GlyphCanvas
                            key={glyph.name}
                            font={font}
                            glyph={glyph}
                            scene={scene}
                        />
                        <TitleBlock font={font} glyph={glyph} />
                        <ToolPalette />
                    </div>
                ) : (
                    <Empty>
                        {font ? "This font has no glyphs yet." : "Checking…"}
                    </Empty>
                )}
            </Centre>
        </>
    );
}

function Toolbar({
    font,
    glyph,
}: {
    font: FontData | null;
    glyph?: GlyphInfo;
}) {
    const layers = useStore((s) => s.layers);
    const toggleLayer = useStore((s) => s.toggleLayer);
    const setGlyph = useStore((s) => s.setGlyph);
    const glyphs = font?.glyphs ?? [];
    const i = glyph ? glyphs.indexOf(glyph) : -1;
    const prev = i > 0 ? glyphs[i - 1] : undefined;
    const next = i >= 0 && i < glyphs.length - 1 ? glyphs[i + 1] : undefined;

    return (
        <>
            <span className={sheet.view}>
                View 02{glyph ? `-${glyph.name}` : ""}
            </span>
            <button
                type="button"
                className={styles.nav}
                disabled={!prev}
                onClick={() => prev && setGlyph(prev.name)}
                title={prev ? `Previous: ${prev.name}` : undefined}
            >
                ‹ {prev?.name}
            </button>
            {glyph && (
                <span className={styles.current}>
                    {glyph.name}
                    {glyph.codepoints[0] !== undefined && (
                        <span className={styles.cp}>
                            U+{hex(glyph.codepoints[0])}
                        </span>
                    )}
                </span>
            )}
            <button
                type="button"
                className={styles.nav}
                disabled={!next}
                onClick={() => next && setGlyph(next.name)}
                title={next ? `Next: ${next.name}` : undefined}
            >
                {next?.name} ›
            </button>
            <span className={sheet.spacer} />
            {LAYERS.map(({ key, label }) => (
                <button
                    type="button"
                    key={key}
                    className={styles.layer}
                    data-on={layers[key] || undefined}
                    onClick={() => toggleLayer(key)}
                >
                    {label}
                </button>
            ))}
        </>
    );
}

function TitleBlock({ font, glyph }: { font: FontData; glyph: GlyphInfo }) {
    const info = useStore((s) => s.lastGood?.font);
    return (
        <div className={styles.titleBlock}>
            <div className={styles.titleHead}>
                {info?.name ?? "Untitled"} · Glyph {glyph.name}
            </div>
            <div>
                <span>CP</span>{" "}
                {glyph.codepoints.length
                    ? glyph.codepoints.map((c) => `U+${hex(c)}`).join(" ")
                    : "—"}
            </div>
            <div>
                <span>EM</span> {font.em}
            </div>
            <div>
                <span>ADV</span>{" "}
                {glyph.fields.advance ? `${glyph.fields.advance} · ` : ""}
                {fmt(glyph.advance)}
            </div>
            <div>
                <span>REV</span> {info?.version ?? "—"}
            </div>
        </div>
    );
}

function ToolPalette() {
    const tool = useStore((s) => s.tool);
    const setTool = useStore((s) => s.setTool);

    // Tool shortcuts, unless the user is typing somewhere.
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.ctrlKey || e.metaKey || e.altKey) return;
            const target = e.target as HTMLElement;
            if (target.closest("input, textarea, select, .cm-editor")) return;
            const hit = TOOLS.find(
                (t) => t.key.toLowerCase() === e.key.toLowerCase(),
            );
            if (hit) setTool(hit.key);
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [setTool]);

    const active = TOOLS.find((t) => t.key === tool);
    return (
        <div className={styles.palette}>
            <span className={styles.paletteLabel}>Tools</span>
            {TOOLS.map((t) => (
                <button
                    type="button"
                    key={t.key}
                    className={styles.tool}
                    data-on={tool === t.key || undefined}
                    title={`${t.name} (${t.key}) · ${t.hint}`}
                    onClick={() => setTool(t.key)}
                >
                    <span className={styles.toolGlyph}>{t.glyph}</span>
                    <span className={styles.toolKey}>{t.key}</span>
                </button>
            ))}
            <div className={styles.active}>
                <span className="label">Active</span>
                <span className={styles.activeName}>{active?.name}</span>
                {tool === "C" ? (
                    <ComponentTarget />
                ) : (
                    <span className={styles.activeHint}>{active?.hint}</span>
                )}
            </div>
        </div>
    );
}

/** The glyph the component tool places. */
function ComponentTarget() {
    const glyphs = useStore((s) => s.font?.glyphs);
    const current = useStore((s) => s.glyph);
    const target = useStore((s) => s.componentTarget);
    const setTarget = useStore((s) => s.setComponentTarget);
    return (
        <select
            className={styles.targetSelect}
            value={target ?? ""}
            aria-label="Glyph to place"
            onChange={(e) => setTarget(e.target.value || null)}
        >
            <option value="">Pick a glyph…</option>
            {glyphs
                ?.filter((g) => g.name !== current)
                .map((g) => (
                    <option key={g.name} value={g.name}>
                        {g.name}
                    </option>
                ))}
        </select>
    );
}

function Inspector({ glyph, scene }: { glyph: GlyphInfo; scene: GlyphScene }) {
    const selection = useStore((s) => s.selection);
    const line = useStore((s) => {
        const sel = s.selection;
        if (!sel) return null;
        return s.text.slice(0, sel.span[0]).split("\n").length;
    });

    const point =
        selection?.kind === "point"
            ? scene.points.find((p) => p.name === selection.name)
            : undefined;
    const cline =
        selection?.kind === "line"
            ? scene.lines.find((l) => l.name === selection.name)
            : undefined;
    const path = selectedPath(scene, selection);
    const segment =
        selection?.kind === "segment" && path
            ? path.segments[Number(selection.name.split("/")[1])]
            : undefined;
    const segmentIndex =
        selection?.kind === "segment"
            ? Number(selection.name.split("/")[1])
            : -1;
    const component =
        selection?.kind === "component"
            ? scene.components[Number(selection.name)]
            : undefined;
    const measure =
        selection?.kind === "measure"
            ? scene.measures.find((m) => m.name === selection.name)
            : undefined;

    let body = (
        <p className="note" style={{ margin: 0 }}>
            Nothing selected. Click a point, path, line or component; or move
            the cursor into a declaration in the source.
        </p>
    );
    if (point) {
        body = (
            <>
                <Title
                    key={`point:${point.name}`}
                    selectionKey={`point:${point.name}`}
                    keyword="let"
                    name={point.name}
                    kind={`point · glyph ${glyph.name}`}
                    renameable
                />
                <div className={styles.expr}>{point.expr}</div>
                <div className={styles.xy}>
                    <span>
                        <i>X</i>
                        {fmt(point.at[0])}
                    </span>
                    <span>
                        <i>Y</i>
                        {fmt(point.at[1])}
                    </span>
                </div>
            </>
        );
    } else if (cline) {
        body = (
            <>
                <Title
                    key={`line:${cline.name}`}
                    selectionKey={`line:${cline.name}`}
                    keyword="let"
                    name={cline.name}
                    kind={`line · glyph ${glyph.name}`}
                    renameable
                />
                <div className={styles.expr}>{cline.expr}</div>
            </>
        );
    } else if (segment && path) {
        body = (
            <>
                <Title
                    key={`segment:${selection?.name}:${segment.name}`}
                    keyword={segment.kind}
                    name={
                        segment.name ??
                        `#${Number(selection?.name.split("/")[1])}`
                    }
                    kind={`segment · path ${pathKey(path)}`}
                    renameable={segment.name !== undefined}
                />
                {segment.to && (
                    <div className={styles.xy}>
                        <span>
                            <i>X</i>
                            {fmt(segment.to[0])}
                        </span>
                        <span>
                            <i>Y</i>
                            {fmt(segment.to[1])}
                        </span>
                    </div>
                )}
            </>
        );
    } else if (path && selection?.kind === "path") {
        body = (
            <Title
                key={`path:${pathKey(path)}`}
                selectionKey={`path:${pathKey(path)}`}
                keyword="path"
                name={pathKey(path)}
                kind={`glyph ${glyph.name}`}
                renameable={path.name !== undefined}
            />
        );
    } else if (component) {
        body = (
            <Title
                keyword="component"
                name={component.glyph}
                kind="double-click to open"
            />
        );
    } else if (measure) {
        body = (
            <Title
                key={`measure:${measure.name}`}
                selectionKey={`measure:${measure.name}`}
                keyword="let"
                name={measure.name}
                kind={`measurement · ${fmt(measure.value)}`}
                renameable
            />
        );
    }

    return (
        <>
            <Section title="Selection" aside={line ? `ln ${line}` : undefined}>
                {body}
            </Section>
            {point && <Drivers point={point.name} />}
            {point && <Constraints point={point.name} scene={scene} />}
            {segment && path && segmentIndex > 0 && (
                <SegmentProps path={path} index={segmentIndex} />
            )}
            {path && <PathProps path={path} />}
        </>
    );
}

/** The literals a drag of `point` would rewrite (plan 5, §1.5): the
 * driver of each axis marked, Tab to cycle, a slider to scrub the first. */
function Drivers({ point }: { point: string }) {
    const info = useStore((s) => s.drivers);
    const drag = useStore((s) => s.drag);
    const version = useStore((s) => (s.doc?.evaluated ? s.doc.version : null));
    const instance = useStore((s) => s.instance);
    const glyph = useStore((s) => s.glyph);

    // Refresh after every evaluated change (not during a drag, which has
    // its own).
    useEffect(() => {
        if (version === null || !instance || !glyph || drag) return;
        let live = true;
        void fetchDrivers(instance, glyph, point).then((next) => {
            if (live) useStore.getState().setDrivers(next);
        });
        return () => {
            live = false;
        };
    }, [point, version, instance, glyph, drag]);

    const shown = drag?.info ?? (info?.target === point ? info : null);
    if (!shown) return null;
    const primary = shown.axis[0] ?? shown.axis[1];
    const axisTitle = shown.track
        ? "track"
        : [shown.axis[0] !== null ? "X" : "", shown.axis[1] !== null ? "Y" : ""]
              .filter(Boolean)
              .join(" · ");
    return (
        <Section
            title={`Drivers${axisTitle ? ` · ${axisTitle}` : ""}`}
            aside="Tab cycles"
        >
            {shown.drivers.length === 0 && (
                <p className="note" style={{ margin: 0 }}>
                    No local literal: this point is set only by top-level
                    values.
                </p>
            )}
            <div className={styles.drivers}>
                {shown.drivers.map((d, i) => {
                    const axes = [
                        shown.axis[0] === i ? "x" : "",
                        shown.axis[1] === i ? "y" : "",
                    ].join("");
                    return (
                        <div
                            // biome-ignore lint/suspicious/noArrayIndexKey: drivers are positional
                            key={i}
                            className={styles.driver}
                            data-on={axes ? true : undefined}
                        >
                            <span className={styles.driverMark}>
                                {axes ? `●${axes}` : "–"}
                            </span>
                            <span>
                                {d.literal} <i>in {d.owner}</i>
                            </span>
                            <span className={styles.driverSens}>
                                {sensitivity(d.sens, d.linear)}
                            </span>
                        </div>
                    );
                })}
            </div>
            {primary !== null && (
                <Scrub
                    key={`${point}:${primary}:${shown.drivers[primary].value}`}
                    point={point}
                    index={primary}
                    literal={shown.drivers[primary].literal}
                    value={shown.drivers[primary].value}
                />
            )}
            {([0, 1] as const).map((a) =>
                shown.axis[a] === null ? (
                    <p key={a} className="note" style={{ margin: 0 }}>
                        <b className={styles.locked}>
                            {a === 0 ? "X" : "Y"} locked
                        </b>{" "}
                        · set by <code>{lockText(shown, a)}</code>. A drag never
                        moves other points or top-level lets.
                    </p>
                ) : null,
            )}
            {shown.anchors.length > 0 && shown.axis.some((a) => a !== null) && (
                <p className={styles.constraintNote}>
                    Placed from {shown.anchors.join(", ")}: drag those directly.
                </p>
            )}
        </Section>
    );
}

/** `×500 x · linear`: how far the point moves per unit of a driver. */
function sensitivity(sens: [number, number], linear: boolean) {
    const parts = [0, 1]
        .filter((a) => Math.abs(sens[a]) > 1e-6)
        .map((a) => `×${fmt(Math.abs(sens[a]))} ${a === 0 ? "x" : "y"}`);
    if (parts.length === 0) return "no effect";
    return `${parts.join(" ")}${linear ? "" : " · curve"}`;
}

/** Scrubs a driver's literal: one gesture from press to release. */
function Scrub({
    point,
    index,
    literal,
    value,
}: {
    point: string;
    index: number;
    literal: string;
    value: number;
}) {
    const places = literal.match(/\.(\d+)/)?.[1].length ?? 0;
    const reach = Math.max(Math.abs(value), 1);
    return (
        <input
            type="range"
            className={styles.scrub}
            aria-label={`Scrub ${literal}`}
            min={value - reach}
            max={value + reach}
            step={10 ** -places}
            defaultValue={value}
            onPointerDown={() => void startDrag(point)}
            onInput={(e) => scrubDrag(index, Number(e.currentTarget.value))}
            onPointerUp={() => void endDrag()}
            onKeyUp={() => void endDrag()}
            onKeyDown={() => {
                if (!dragging()) void startDrag(point);
            }}
        />
    );
}

/** The relationship tools (plan 5, §1.4) for the selected point. */
function Constraints({ point, scene }: { point: string; scene: GlyphScene }) {
    const relate = useStore((s) => s.relate);
    const reasons = RELATE_TOOLS.map((t) => disabledReason(t.kind, scene));
    const firstReason = reasons.find(Boolean);
    return (
        <Section title="Constraints" aside={relate ? "Esc cancels" : undefined}>
            <div className={styles.constraints}>
                {RELATE_TOOLS.map((t, i) => (
                    <button
                        type="button"
                        key={t.kind}
                        className={styles.constraint}
                        disabled={!!reasons[i]}
                        data-on={relate?.kind === t.kind || undefined}
                        title={reasons[i] ?? `${t.name}: ${t.prompt}`}
                        onClick={() => startRelate(t.kind, point)}
                    >
                        <b>{t.kind}</b>
                        {t.name}
                    </button>
                ))}
            </div>
            {firstReason && (
                <p className={styles.constraintNote}>{firstReason}</p>
            )}
        </Section>
    );
}

const KINDS: SegmentKind[] = ["line", "quad", "cube", "arc"];

/** A segment's kind (plan 5, §1.4: converting seeds new control points
 * from the current shape) and, for an arc, its sweep. */
function SegmentProps({ path, index }: { path: PathInfo; index: number }) {
    const segment = path.segments[index];
    if (!segment || segment.kind === "start") return null;
    return (
        <Section title={`Segment · ${segment.name ?? `#${index}`}`}>
            <div className={styles.props}>
                <span className={sheet.key}>Kind</span>
                <Segmented
                    options={KINDS}
                    value={segment.kind}
                    onPick={(kind) =>
                        setSegmentKind(path, index, kind as SegmentKind)
                    }
                />
                {segment.arc && (
                    <>
                        <span className={sheet.key}>Sweep</span>
                        <Segmented
                            options={["ccw", "cw"]}
                            value={segment.arc.sweep}
                            onPick={(sweep) =>
                                void setSegmentField(path, index, "sweep", {
                                    type: "str",
                                    value: sweep,
                                })
                            }
                        />
                        {segment.arc.large !== undefined && (
                            <>
                                <span className={sheet.key}>Large</span>
                                <Segmented
                                    options={["false", "true"]}
                                    value={String(segment.arc.large)}
                                    onPick={(large) =>
                                        void setSegmentField(
                                            path,
                                            index,
                                            "large",
                                            {
                                                type: "bool",
                                                value: large === "true",
                                            },
                                        )
                                    }
                                />
                            </>
                        )}
                    </>
                )}
            </div>
        </Section>
    );
}

/** The selection's keyword and name. A renameable name is a button that
 * opens an inline field: Enter renames (plan 5, §1.3), Escape cancels. */
function Title({
    keyword,
    name,
    kind,
    renameable,
    selectionKey,
}: {
    keyword: string;
    name: string;
    kind: string;
    renameable?: boolean;
    /** `kind:name`: the field opens by itself when an edit just created
     * this declaration (plan 5, §1.3: Enter keeps the placeholder). */
    selectionKey?: string;
}) {
    const [draft, setDraft] = useState<string | null>(() =>
        renameable &&
        selectionKey !== undefined &&
        useStore.getState().renaming === selectionKey
            ? name
            : null,
    );
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (selectionKey && useStore.getState().renaming === selectionKey) {
            useStore.getState().setRenaming(null);
        }
    }, [selectionKey]);

    const commit = async () => {
        if (draft === null) return;
        const next = draft.trim();
        if (next === "" || next === name) {
            setDraft(null);
            return;
        }
        const result = await renameSelection(next);
        if (result?.status === "invalid") {
            setError(result.message);
        } else {
            setDraft(null);
            setError(null);
        }
    };

    return (
        <>
            <div className={styles.title}>
                <span className={styles.keyword}>{keyword}</span>
                {draft !== null ? (
                    <input
                        className={styles.rename}
                        value={draft}
                        spellCheck={false}
                        // biome-ignore lint/a11y/noAutofocus: opened by the user's click
                        autoFocus
                        aria-label={`New name for ${name}`}
                        onChange={(e) => {
                            setDraft(e.target.value);
                            setError(null);
                        }}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") void commit();
                            if (e.key === "Escape") {
                                setDraft(null);
                                setError(null);
                            }
                        }}
                        onBlur={() => {
                            setDraft(null);
                            setError(null);
                        }}
                    />
                ) : renameable ? (
                    <button
                        type="button"
                        className={styles.name}
                        title="Rename"
                        onClick={() => setDraft(name)}
                    >
                        {name}
                    </button>
                ) : (
                    <span className={styles.name}>{name}</span>
                )}
                <span className={styles.kind}>{kind}</span>
            </div>
            {error && <p className={styles.renameError}>▲ {error}</p>}
        </>
    );
}

/** The path the selection is in: the path itself, a segment's path, or a
 * path whose segment ends at the selected point. */
function selectedPath(
    scene: GlyphScene,
    selection: ReturnType<typeof useStore.getState>["selection"],
) {
    if (!selection) return undefined;
    if (selection.kind === "path" || selection.kind === "segment") {
        const key = selection.name.split("/")[0];
        return scene.paths.find((p) => pathKey(p) === key);
    }
    if (selection.kind === "point") {
        return scene.paths.find((p) =>
            p.segments.some((s) => s.toRef === selection.name),
        );
    }
    return undefined;
}

/** A path's rendering fields (plan 5, §1.4). Each control is one edit. */
function PathProps({ path }: { path: PathInfo }) {
    const caps = path.caps;
    const capsLabel =
        caps && caps[0] !== caps[1] ? `${caps[0]} / ${caps[1]}` : caps?.[0];
    const str = (value: string) => ({ type: "str" as const, value });
    return (
        <Section title={`Path · ${pathKey(path)}`}>
            <div className={styles.props}>
                <span className={sheet.key}>Stroke</span>
                <ExprField
                    key={path.stroke ?? ""}
                    value={path.stroke ?? ""}
                    placeholder="none · construction"
                    onCommit={(text) =>
                        text === ""
                            ? removePathField(path, "stroke")
                            : setPathField(path, "stroke", {
                                  type: "expr",
                                  value: text,
                              })
                    }
                />
                <span className={sheet.key}>Caps</span>
                <Segmented
                    options={["butt", "round", "square"]}
                    value={capsLabel ?? "butt"}
                    onPick={(cap) => void setPathField(path, "caps", str(cap))}
                />
                <span className={sheet.key}>Joins</span>
                <Segmented
                    options={["miter", "round", "bevel"]}
                    value={path.joins}
                    onPick={(join) =>
                        void setPathField(path, "joins", str(join))
                    }
                />
                <span className={sheet.key}>Fill</span>
                <Segmented
                    options={["off", "on"]}
                    value={path.fill ? "on" : "off"}
                    onPick={(fill) => void setFill(path, fill === "on")}
                />
            </div>
        </Section>
    );
}

/** One choice of several; `onPick` makes it an edit. */
function Segmented({
    options,
    value,
    onPick,
}: {
    options: string[];
    value?: string;
    onPick?: (option: string) => void;
}) {
    return (
        <span className={styles.segmented}>
            {options.map((o) => (
                <button
                    type="button"
                    key={o}
                    data-on={o === value || undefined}
                    disabled={!onPick}
                    onClick={() => o !== value && onPick?.(o)}
                >
                    {o}
                </button>
            ))}
            {value && !options.includes(value) && (
                <button type="button" data-on disabled>
                    {value}
                </button>
            )}
        </span>
    );
}

/** An expression typed into the inspector, spliced as-is on Enter or
 * blur (plan 5, §2.4); Escape reverts. */
function ExprField({
    value,
    placeholder,
    onCommit,
}: {
    value: string;
    placeholder?: string;
    onCommit: (text: string) => Promise<unknown>;
}) {
    const [text, setText] = useState(value);
    const commit = () => {
        const next = text.trim();
        if (next !== value) void onCommit(next);
    };
    return (
        <input
            className={styles.exprField}
            value={text}
            placeholder={placeholder}
            spellCheck={false}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") {
                    setText(value);
                    e.currentTarget.blur();
                }
            }}
            onBlur={commit}
        />
    );
}
