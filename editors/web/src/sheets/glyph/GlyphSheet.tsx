import { useEffect, useState } from "react";
import type {
    FontData,
    GlyphInfo,
    GlyphScene,
    PathInfo,
} from "../../engine/types.ts";
import { fmt, hex, pathKey } from "../../font/lookup.ts";
import { renameSelection } from "../../state/actions.ts";
import type { Tool } from "../../state/store.ts";
import { LAYERS, useStore } from "../../state/store.ts";
import { Centre, Empty, LeftColumn, Section, sheet } from "../../ui/Sheet.tsx";
import { GlyphCanvas } from "./GlyphCanvas.tsx";
import styles from "./GlyphSheet.module.css";

const TOOLS: { key: Tool; glyph: string; name: string }[] = [
    { key: "V", glyph: "↖", name: "Select / drag" },
    { key: "P", glyph: "✎", name: "Path" },
    { key: ".", glyph: "+", name: "Constr. point" },
    { key: "L", glyph: "/", name: "Line through" },
    { key: "G", glyph: "┼", name: "Guide" },
    { key: "M", glyph: "↔", name: "Measure" },
    { key: "C", glyph: "◫", name: "Component" },
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
                    title={`${t.name} (${t.key}) · editing arrives in W4`}
                    onClick={() => setTool(t.key)}
                >
                    <span className={styles.toolGlyph}>{t.glyph}</span>
                    <span className={styles.toolKey}>{t.key}</span>
                </button>
            ))}
            <div className={styles.active}>
                <span className="label">Active</span>
                <span className={styles.activeName}>{active?.name}</span>
            </div>
        </div>
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
    const component =
        selection?.kind === "component"
            ? scene.components[Number(selection.name)]
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
    }

    return (
        <>
            <Section title="Selection" aside={line ? `ln ${line}` : undefined}>
                {body}
            </Section>
            {path && <PathProps path={path} />}
        </>
    );
}

/** The selection's keyword and name. A renameable name is a button that
 * opens an inline field: Enter renames (plan 5, §1.3), Escape cancels. */
function Title({
    keyword,
    name,
    kind,
    renameable,
}: {
    keyword: string;
    name: string;
    kind: string;
    renameable?: boolean;
}) {
    const [draft, setDraft] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

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

function PathProps({ path }: { path: PathInfo }) {
    const caps = path.caps;
    const capsLabel =
        caps && caps[0] !== caps[1] ? `${caps[0]} / ${caps[1]}` : caps?.[0];
    return (
        <Section title={`Path · ${pathKey(path)}`}>
            <div className={styles.props}>
                <span className={sheet.key}>Stroke</span>
                <span>{path.stroke ?? "—"}</span>
                <span className={sheet.key}>Caps</span>
                <Segmented
                    options={["butt", "round", "square"]}
                    value={capsLabel}
                />
                <span className={sheet.key}>Joins</span>
                <Segmented
                    options={["miter", "round", "bevel"]}
                    value={path.joins}
                />
                <span className={sheet.key}>Fill</span>
                <span style={{ color: path.fill ? undefined : "var(--mid)" }}>
                    {path.fill ? "on" : "off"} ·{" "}
                    {path.closed ? "closed" : "open"}
                </span>
                <span className={sheet.key}>Enabled</span>
                <span>{path.enabled ? "✓ true" : "false"}</span>
                {path.follows && (
                    <>
                        <span className={sheet.key}>Follows</span>
                        <span>{path.follows}</span>
                    </>
                )}
            </div>
        </Section>
    );
}

/** Read-only until W4 wires the edits. */
function Segmented({ options, value }: { options: string[]; value?: string }) {
    return (
        <span className={styles.segmented}>
            {options.map((o) => (
                <span key={o} data-on={o === value || undefined}>
                    {o}
                </span>
            ))}
            {value && !options.includes(value) && <span data-on>{value}</span>}
        </span>
    );
}
