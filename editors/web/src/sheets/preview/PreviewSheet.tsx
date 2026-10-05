import { useEffect, useRef, useState } from "react";
import type { FontData } from "../../engine/types.ts";
import { verticalExtent } from "../../font/lookup.ts";
import { useStore } from "../../state/store.ts";
import { Centre, Empty, LeftColumn, Section, sheet } from "../../ui/Sheet.tsx";
import { caretX, indexAt, type Line, layout, lineOf } from "./layout.ts";
import styles from "./PreviewSheet.module.css";

/** Sample texts; "Every glyph" is built from the font itself. */
const PRESETS: { label: string; text: string }[] = [
    { label: "Pangram", text: "The quick brown fox jumps over the lazy dog." },
    { label: "Caps", text: "THE QUICK BROWN FOX\nJUMPS OVER THE LAZY DOG" },
    { label: "Hamburgefonstiv", text: "Hamburgefonstiv HAMBURGEFONSTIV" },
    {
        label: "Alphabet",
        text: "ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz",
    },
    { label: "Digits", text: "0123456789 +-*/=%$#&!?.,;:'\"()" },
];

/** Every encoded glyph's character, in declaration order. */
function everyGlyph(font: FontData): string {
    return font.glyphs
        .filter((g) => g.codepoints.length > 0)
        .map((g) => String.fromCodePoint(g.codepoints[0]))
        .join("");
}

/** The page's padding, in pixels (matches `.page` in the CSS). */
const PAD_X = 32;
const PAD_Y = 24;

export function PreviewSheet() {
    const font = useStore((s) => s.font);
    const text = useStore((s) => s.previewText);
    const setText = useStore((s) => s.setPreviewText);
    const size = useStore((s) => s.previewSize);
    const setSize = useStore((s) => s.setPreviewSize);
    const kern = useStore((s) => s.previewKern);
    const setKern = useStore((s) => s.setPreviewKern);
    const metrics = useStore((s) => s.previewMetrics);
    const setMetrics = useStore((s) => s.setPreviewMetrics);

    const toolbar = (
        <>
            <span className={sheet.view}>View 05 · Preview</span>
            <span className={sheet.toolLabel}>Size</span>
            <input
                type="range"
                className={styles.size}
                min={24}
                max={200}
                value={size}
                aria-label="Preview size in pixels"
                onChange={(e) => setSize(Number(e.target.value))}
            />
            <span className={styles.sizeValue}>{size} px</span>
            <span className={sheet.spacer} />
            <button
                type="button"
                className={sheet.toolButton}
                data-on={kern || undefined}
                title="Apply the font's kerning"
                onClick={() => setKern(!kern)}
            >
                Kerning {kern ? "on" : "off"}
            </button>
            <button
                type="button"
                className={sheet.toolButton}
                data-on={metrics || undefined}
                title="Draw the baseline and metric lines"
                onClick={() => setMetrics(!metrics)}
            >
                Metrics {metrics ? "on" : "off"}
            </button>
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

    const missing = new Set(
        layout(font, text, Number.POSITIVE_INFINITY, false).flatMap((l) =>
            l.items.filter((i) => !i.glyph && i.text.trim()).map((i) => i.text),
        ),
    );

    return (
        <>
            <LeftColumn>
                <Section title="Sample text" flush>
                    <div className={styles.presets}>
                        {PRESETS.map((p) => (
                            <button
                                type="button"
                                key={p.label}
                                className={styles.preset}
                                onClick={() => setText(p.text)}
                            >
                                {p.label}
                            </button>
                        ))}
                        <button
                            type="button"
                            className={styles.preset}
                            onClick={() => setText(everyGlyph(font))}
                        >
                            Every glyph
                        </button>
                    </div>
                    <p className={styles.note}>
                        Click the preview and type. Presets replace the text.
                    </p>
                </Section>
                {missing.size > 0 && (
                    <Section title="Not in the font" aside={`${missing.size}`}>
                        <p className={styles.missing}>
                            {[...missing].join(" ")}
                        </p>
                    </Section>
                )}
            </LeftColumn>
            <Centre toolbar={toolbar}>
                <Page
                    font={font}
                    text={text}
                    setText={setText}
                    scale={size / font.em}
                    kern={kern}
                    metrics={metrics}
                />
            </Centre>
        </>
    );
}

/**
 * The setting, editable in place: a hidden textarea holds the text and
 * the focus (so typing, paste, undo, and IME composition all behave as
 * in any text field), and the page draws its caret and selection on the
 * set glyphs. Clicks, drags, and Up/Down/Home/End follow the lines as
 * drawn.
 */
function Page({
    font,
    text,
    setText,
    scale,
    kern,
    metrics,
}: {
    font: FontData;
    text: string;
    setText: (text: string) => void;
    scale: number;
    kern: boolean;
    metrics: boolean;
}) {
    const pageRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const [width, setWidth] = useState(800);
    const [focused, setFocused] = useState(false);
    const [sel, setSel] = useState<[number, number]>([
        text.length,
        text.length,
    ]);
    /** Where a mouse drag began, as a text offset. */
    const dragFrom = useRef<number | null>(null);

    useEffect(() => {
        const el = pageRef.current;
        if (!el) return;
        const observer = new ResizeObserver(([entry]) =>
            setWidth(entry.contentRect.width),
        );
        observer.observe(el);
        return () => observer.disconnect();
    }, []);

    const [descender, ascender] = verticalExtent(font);
    const lineHeight = (ascender - descender) * scale;
    const lines = layout(font, text, Math.max(width / scale, font.em), kern);

    /** Mirrors the textarea's selection, which the page draws. */
    const sync = () => {
        const input = inputRef.current;
        if (input) setSel([input.selectionStart, input.selectionEnd]);
    };

    /** The text offset under a mouse event. */
    const offsetAt = (e: React.MouseEvent) => {
        const page = pageRef.current;
        if (!page) return 0;
        const box = page.getBoundingClientRect();
        const y = e.clientY - box.top + page.scrollTop - PAD_Y;
        const n = Math.min(
            lines.length - 1,
            Math.max(0, Math.floor(y / lineHeight)),
        );
        const x = (e.clientX - box.left + page.scrollLeft - PAD_X) / scale;
        return indexAt(lines[n], x);
    };

    const select = (anchor: number, head: number) => {
        const input = inputRef.current;
        if (!input) return;
        input.focus();
        input.setSelectionRange(
            Math.min(anchor, head),
            Math.max(anchor, head),
            head < anchor ? "backward" : "forward",
        );
        sync();
    };

    /** Up/Down/Home/End by the lines as drawn, not the textarea's own. */
    const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        const input = e.currentTarget;
        const backward = input.selectionDirection === "backward";
        const head = backward ? input.selectionStart : input.selectionEnd;
        const anchor = backward ? input.selectionEnd : input.selectionStart;
        const n = lineOf(lines, head);
        let to: number | null = null;
        if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            const target = n + (e.key === "ArrowUp" ? -1 : 1);
            if (target < 0) to = 0;
            else if (target >= lines.length) to = text.length;
            else to = indexAt(lines[target], caretX(lines[n], head));
        } else if (e.key === "Home" && !e.ctrlKey && !e.metaKey) {
            to = lines[n].start;
        } else if (e.key === "End" && !e.ctrlKey && !e.metaKey) {
            const line = lines[n];
            // A wrapped line ends where the next begins; stay on this one.
            to = lines[n + 1]?.start === line.end ? line.end - 1 : line.end;
        }
        if (to === null) return;
        e.preventDefault();
        select(e.shiftKey ? anchor : to, to);
    };

    const [from, to] = sel;
    const caretLine = lineOf(lines, from === to ? to : sel[1]);
    const caretLeft =
        PAD_X + caretX(lines[caretLine], from === to ? to : sel[1]) * scale;
    const caretTop = PAD_Y + caretLine * lineHeight;

    return (
        // biome-ignore lint/a11y/noStaticElementInteractions: the textarea inside takes the keyboard; this maps clicks onto it
        <div
            ref={pageRef}
            className={styles.page}
            data-focused={focused || undefined}
            onMouseDown={(e) => {
                if (e.button !== 0) return;
                e.preventDefault();
                const at = offsetAt(e);
                dragFrom.current = e.shiftKey ? sel[0] : at;
                select(dragFrom.current, at);
            }}
            onMouseMove={(e) => {
                if (dragFrom.current === null || !(e.buttons & 1)) return;
                select(dragFrom.current, offsetAt(e));
            }}
            onMouseUp={() => {
                dragFrom.current = null;
            }}
        >
            <textarea
                ref={inputRef}
                className={styles.input}
                value={text}
                spellCheck={false}
                autoCapitalize="off"
                autoComplete="off"
                aria-label="Preview text"
                // Kept at the caret, so an IME's window opens beside it.
                style={{ left: caretLeft, top: caretTop, height: lineHeight }}
                onChange={(e) => {
                    setText(e.target.value);
                    sync();
                }}
                onSelect={sync}
                onKeyDown={onKeyDown}
                onFocus={() => {
                    setFocused(true);
                    sync();
                }}
                onBlur={() => setFocused(false)}
            />
            {lines.map((line, i) => {
                // The selection's part of this line, in font units.
                const a = Math.max(from, line.start);
                const b = Math.min(to, line.end);
                const span =
                    from !== to && a <= b && (a < b || from < line.start)
                        ? [
                              caretX(line, a),
                              b >= line.end ? line.width : caretX(line, b),
                          ]
                        : null;
                return (
                    <PreviewLine
                        // biome-ignore lint/suspicious/noArrayIndexKey: lines are positional
                        key={i}
                        font={font}
                        line={line}
                        scale={scale}
                        metrics={metrics}
                        selected={span}
                    />
                );
            })}
            {focused && from === to && (
                <span
                    className={styles.caret}
                    style={{
                        left: caretLeft,
                        top: caretTop,
                        height: lineHeight,
                    }}
                />
            )}
        </div>
    );
}

function PreviewLine({
    font,
    line,
    scale,
    metrics,
    selected,
}: {
    font: FontData;
    line: Line;
    scale: number;
    metrics: boolean;
    /** The selected stretch, `[x0, x1]` in font units. */
    selected: number[] | null;
}) {
    const [descender, ascender] = verticalExtent(font);
    const height = ascender - descender;
    const width = Math.max(line.width, 1);
    const cap =
        font.metrics.find((m) => m.name === "capHeight")?.y ?? font.em * 0.7;
    return (
        <svg
            className={styles.line}
            width={width * scale}
            height={height * scale}
            viewBox={`0 ${-ascender} ${width} ${height}`}
        >
            <title>{line.items.map((item) => item.text).join("")}</title>
            {selected && (
                <rect
                    x={selected[0]}
                    y={-ascender}
                    width={Math.max(selected[1] - selected[0], font.em * 0.1)}
                    height={height}
                    className={styles.selection}
                />
            )}
            {metrics &&
                font.metrics.map((m) =>
                    m.y === undefined ? null : (
                        <line
                            key={m.name}
                            x1={0}
                            x2={width}
                            y1={-m.y}
                            y2={-m.y}
                            className={
                                m.y === 0 ? styles.baseline : styles.metric
                            }
                        />
                    ),
                )}
            {line.items.map((item, i) =>
                item.glyph ? (
                    <path
                        // biome-ignore lint/suspicious/noArrayIndexKey: a line can repeat glyphs
                        key={i}
                        d={item.glyph.outline}
                        transform={`translate(${item.x} 0) scale(1 -1)`}
                        className={styles.ink}
                    >
                        <title>{item.glyph.name}</title>
                    </path>
                ) : item.text.trim() ? (
                    <rect
                        // biome-ignore lint/suspicious/noArrayIndexKey: as above
                        key={i}
                        x={item.x + item.advance * 0.1}
                        y={-cap}
                        width={item.advance * 0.8}
                        height={cap}
                        className={styles.notdef}
                    >
                        <title>{`no glyph for “${item.text}”`}</title>
                    </rect>
                ) : null,
            )}
        </svg>
    );
}
