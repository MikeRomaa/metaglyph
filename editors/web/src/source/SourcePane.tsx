import {
    defaultKeymap,
    history,
    historyKeymap,
    indentWithTab,
    undoDepth,
} from "@codemirror/commands";
import { bracketMatching, indentUnit } from "@codemirror/language";
import type { Diagnostic } from "@codemirror/lint";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import {
    highlightSelectionMatches,
    search,
    searchKeymap,
} from "@codemirror/search";
import type { Extension } from "@codemirror/state";
import { EditorState, Transaction } from "@codemirror/state";
import {
    drawSelection,
    EditorView,
    highlightActiveLine,
    highlightActiveLineGutter,
    keymap,
    lineNumbers,
} from "@codemirror/view";
import { useEffect, useRef } from "react";
import { check, onResult } from "../engine/client.ts";
import type { Sheet } from "../state/store.ts";
import { useStore } from "../state/store.ts";
import { Resizer, usePanelWidth } from "../ui/Resizer.tsx";
import { mgCompletion } from "./complete.ts";
import { setEditorView } from "./editor.ts";
import { flashExtension } from "./flash.ts";
import { formatSource } from "./format.ts";
import { mgHighlight, mgLanguage } from "./mgLanguage.ts";
import styles from "./SourcePane.module.css";
import { highlightExtension, selectionAt, setHighlight } from "./sync.ts";
import { mgTheme } from "./theme.ts";

function extensions(): Extension[] {
    return [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        // Ctrl+D adds the next occurrence as another cursor.
        EditorState.allowMultipleSelections.of(true),
        search({ top: true }),
        highlightSelectionMatches(),
        history(),
        bracketMatching(),
        indentUnit.of("    "),
        EditorState.tabSize.of(4),
        mgLanguage,
        mgHighlight,
        mgCompletion,
        mgTheme,
        lintGutter(),
        highlightExtension(),
        flashExtension(),
        // Ctrl+F find/replace, F3 next, Ctrl+D next occurrence, Ctrl+Shift+L
        // every occurrence, Ctrl+Alt+G go to line.
        keymap.of([
            {
                key: "Shift-Alt-f",
                run: () => {
                    void formatSource();
                    return true;
                },
            },
            ...searchKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            indentWithTab,
        ]),
        EditorView.updateListener.of((update) => {
            const store = useStore.getState();
            if (update.docChanged) {
                const text = update.state.doc.toString();
                const version = store.version + 1;
                store.setText(text, version);
                check(text, version);
                // The last change's kind, for the footer: a canvas edit's
                // label (`mg.rename` → `rename`), or typing.
                for (const tr of update.transactions) {
                    if (!tr.docChanged) continue;
                    const event = tr.annotation(Transaction.userEvent);
                    if (event?.startsWith("mg."))
                        store.setLastOp(event.slice(3));
                    else if (
                        event?.startsWith("input") ||
                        event?.startsWith("delete")
                    ) {
                        store.setLastOp("typing");
                    }
                }
            }
            if (update.docChanged || update.selectionSet) {
                const line = update.state.doc.lineAt(
                    update.state.selection.main.head,
                ).number;
                store.setCursor(line, undoDepth(update.state));
            }
            // Moving the cursor into a declaration selects it (plan 5, §2.2).
            if (update.transactions.some((tr) => tr.isUserEvent("select"))) {
                const hit = selectionAt(update.state.selection.main.head);
                if (!hit) return;
                if (hit.glyph && hit.glyph !== store.glyph) {
                    store.setGlyph(hit.glyph);
                    return;
                }
                if (hit.kern !== undefined) store.setKern(hit.kern);
                store.select(hit.selection);
            }
        }),
    ];
}

/** The declaration a sheet is about: the active glyph on the glyph and
 * spacing sheets. */
function contextSpan(
    sheet: Sheet,
    s: ReturnType<typeof useStore.getState>,
): [number, number] | null {
    if (sheet !== 2 && sheet !== 3) return null;
    return s.font?.glyphs.find((g) => g.name === s.glyph)?.span ?? null;
}

export function SourcePane() {
    const host = useRef<HTMLDivElement>(null);
    const view = useRef<EditorView | null>(null);
    const epoch = useStore((s) => s.epoch);
    const fileName = useStore((s) => s.fileName);
    const cursorLine = useStore((s) => s.cursorLine);
    const depth = useStore((s) => s.undoDepth);
    const saved = useStore((s) => s.saved);
    const doc = useStore((s) => s.doc);
    const lastOp = useStore((s) => s.lastOp);

    useEffect(() => {
        if (!host.current) return;
        const v = new EditorView({ parent: host.current });
        view.current = v;
        setEditorView(v);
        onResult((result) => {
            useStore.getState().applyResult(result);
            const state = result.doc;
            if (!state || state.version !== useStore.getState().version) return;
            const length = v.state.doc.length;
            const diagnostics: Diagnostic[] = state.diagnostics.map((d) => ({
                from: Math.min(d.from, length),
                to: Math.min(d.to, length),
                severity: d.severity,
                source: d.code,
                message: d.message,
            }));
            v.dispatch(setDiagnostics(v.state, diagnostics));
        });

        // Highlight the selection and the sheet's glyph; scroll to a
        // selection made on the canvas. Deferred: the store can change from
        // inside an editor update, where dispatching is not allowed.
        const unsubscribe = useStore.subscribe((s, prev) => {
            if (
                s.selection === prev.selection &&
                s.glyph === prev.glyph &&
                s.font === prev.font &&
                s.sheet === prev.sheet &&
                s.reveal === prev.reveal
            ) {
                return;
            }
            // A canvas selection, or an explicit reveal (a glyph opened
            // from 01), scrolls the text to it.
            const target =
                s.reveal !== prev.reveal && s.reveal
                    ? s.reveal.span
                    : s.selection &&
                        s.selection !== prev.selection &&
                        s.selection.origin === "canvas"
                      ? s.selection.span
                      : null;
            queueMicrotask(() => {
                const length = v.state.doc.length;
                v.dispatch({
                    effects: [
                        setHighlight.of({
                            selected: s.selection?.span ?? null,
                            context: contextSpan(s.sheet, s),
                        }),
                        ...(target
                            ? [
                                  EditorView.scrollIntoView(
                                      Math.min(target[0], length),
                                      { y: "center" },
                                  ),
                              ]
                            : []),
                    ],
                });
            });
        });
        return () => {
            unsubscribe();
            setEditorView(null);
            v.destroy();
        };
    }, []);

    useEffect(() => {
        const v = view.current;
        if (!v || epoch === 0) return;
        const { initialText, version } = useStore.getState();
        v.setState(
            EditorState.create({ doc: initialText, extensions: extensions() }),
        );
        useStore.getState().setCursor(1, 0);
        check(initialText, version);
    }, [epoch]);

    const [width, setWidth, initial] = usePanelWidth("source", 25, 16, 60);

    const errors =
        doc?.diagnostics.filter((d) => d.severity === "error").length ?? 0;
    const problems = doc?.diagnostics.length ?? 0;

    return (
        <section className={styles.pane} style={{ width: `${width}rem` }}>
            <Resizer
                edge="left"
                width={width}
                onResize={setWidth}
                initial={initial}
            />
            <header className={styles.head}>
                <span className={styles.title}>Source</span>
                <span className={styles.status}>
                    {fileName} · ln {cursorLine} · {saved ? "saved" : "unsaved"}
                </span>
                <button
                    type="button"
                    className={styles.format}
                    title="Format the source as `mg fmt` does (Shift+Alt+F)"
                    onClick={() => void formatSource()}
                >
                    Format
                </button>
            </header>
            <div ref={host} className={styles.editor} />
            <footer className={styles.foot}>
                <span
                    className={styles.problems}
                    style={{
                        color:
                            problems === 0
                                ? "var(--str)"
                                : errors > 0
                                  ? "var(--err)"
                                  : "var(--acc)",
                    }}
                >
                    {problems === 0
                        ? "✓ 0 problems"
                        : `▲ ${problems} problem${problems === 1 ? "" : "s"}`}
                </span>
                <span className={styles.undo}>
                    undo ·{" "}
                    {lastOp === null
                        ? "—"
                        : lastOp === "typing"
                          ? "typing"
                          : `mg.${lastOp}`}
                    {" · "}
                    {depth} step{depth === 1 ? "" : "s"}
                </span>
            </footer>
        </section>
    );
}
