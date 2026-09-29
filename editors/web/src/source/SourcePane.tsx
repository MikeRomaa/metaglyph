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
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import {
    drawSelection,
    EditorView,
    highlightActiveLine,
    highlightActiveLineGutter,
    keymap,
    lineNumbers,
} from "@codemirror/view";
import { useEffect, useRef } from "react";
import { check, onDocState } from "../engine/client.ts";
import { useStore } from "../state/store.ts";
import { mgHighlight, mgLanguage } from "./mgLanguage.ts";
import styles from "./SourcePane.module.css";
import { mgTheme } from "./theme.ts";

function extensions(): Extension[] {
    return [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        history(),
        bracketMatching(),
        indentUnit.of("    "),
        EditorState.tabSize.of(4),
        mgLanguage,
        mgHighlight,
        mgTheme,
        lintGutter(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        EditorView.updateListener.of((update) => {
            const store = useStore.getState();
            if (update.docChanged) {
                const text = update.state.doc.toString();
                const version = store.version + 1;
                store.setText(text, version);
                check(text, version);
            }
            if (update.docChanged || update.selectionSet) {
                const line = update.state.doc.lineAt(
                    update.state.selection.main.head,
                ).number;
                store.setCursor(line, undoDepth(update.state));
            }
        }),
    ];
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

    useEffect(() => {
        if (!host.current) return;
        const v = new EditorView({ parent: host.current });
        view.current = v;
        onDocState((state) => {
            useStore.getState().setDoc(state);
            if (state.version !== useStore.getState().version) return;
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
        return () => v.destroy();
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

    const errors =
        doc?.diagnostics.filter((d) => d.severity === "error").length ?? 0;
    const problems = doc?.diagnostics.length ?? 0;

    return (
        <section className={styles.pane}>
            <header className={styles.head}>
                <span className={styles.title}>Source</span>
                <span className={styles.status}>
                    {fileName} · ln {cursorLine} · {saved ? "saved" : "unsaved"}
                </span>
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
                    undo · {depth} step{depth === 1 ? "" : "s"}
                </span>
            </footer>
        </section>
    );
}
