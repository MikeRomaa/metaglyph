import { useEffect, useState } from "react";
import type {
    FontData,
    GlyphInfo,
    GlyphScene,
    PathInfo,
    SegmentKind,
} from "../../engine/types.ts";
import { fmt, hex, pathKey } from "../../font/lookup.ts";
import {
    duplicateFollower,
    removePathField,
    renameSelection,
    setFill,
    setPathField,
    setSegmentField,
    setSegmentKind,
} from "../../state/actions.ts";
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
            {segment && path && segmentIndex > 0 && (
                <SegmentProps path={path} index={segmentIndex} />
            )}
            {path && <PathProps path={path} />}
        </>
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
                <span className={sheet.key}>Enabled</span>
                <Segmented
                    options={["false", "true"]}
                    value={String(path.enabled)}
                    onPick={(on) =>
                        void (on === "true"
                            ? removePathField(path, "enabled")
                            : setPathField(path, "enabled", {
                                  type: "bool",
                                  value: false,
                              }))
                    }
                />
                {path.follows && (
                    <>
                        <span className={sheet.key}>Follows</span>
                        <span>{path.follows}</span>
                    </>
                )}
            </div>
            {path.name && !path.follows && (
                <button
                    type="button"
                    className={styles.action}
                    onClick={() => void duplicateFollower(path)}
                >
                    Duplicate as follower
                </button>
            )}
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
