import { EditorView } from "@codemirror/view";

// The design's source pane: Plex Mono 11.5/17, 34px faint line numbers,
// a 3px gutter stripe, --hi for the current line (all in rem here).
export const mgTheme = EditorView.theme({
    "&": {
        height: "100%",
        background: "var(--panel2)",
        color: "var(--mid)",
        fontSize: "0.71875rem",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": {
        fontFamily: "var(--font-mono)",
        lineHeight: "1.0625rem",
    },
    ".cm-content": { padding: "0.25rem 0", caretColor: "var(--acc)" },
    ".cm-cursor": { borderLeftColor: "var(--acc)", borderLeftWidth: "2px" },
    ".cm-gutters": {
        background: "var(--panel2)",
        border: "none",
        color: "var(--faint)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
        minWidth: "2.3125rem",
        padding: "0 0.625rem 0 0",
        borderLeft: "0.1875rem solid transparent",
    },
    ".cm-activeLine": { background: "var(--hi)" },
    ".cm-activeLineGutter": {
        background: "var(--hi)",
        color: "var(--ink)",
    },
    ".cm-activeLineGutter.cm-gutterElement": { borderLeftColor: "var(--acc)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection":
        {
            background: "var(--tok) !important",
        },
    ".cm-lintRange-error": {
        backgroundImage: "none",
        textDecoration: "underline wavy var(--err)",
        textUnderlineOffset: "0.1875rem",
    },
    ".cm-lintRange-warning": {
        backgroundImage: "none",
        textDecoration: "underline wavy var(--acc)",
        textUnderlineOffset: "0.1875rem",
    },
    ".cm-tooltip": {
        background: "var(--bg)",
        color: "var(--ink)",
        border: "1.5px solid var(--ink)",
        boxShadow: "0.1875rem 0.1875rem 0 var(--ink)",
        borderRadius: "0",
    },
    ".cm-diagnostic": {
        fontFamily: "var(--font-mono)",
        fontSize: "0.6875rem",
        whiteSpace: "pre-wrap",
        padding: "0.25rem 0.5rem",
    },
    ".cm-diagnostic-error": { borderLeft: "0.1875rem solid var(--err)" },
    ".cm-diagnostic-warning": { borderLeft: "0.1875rem solid var(--acc)" },
    ".cm-lint-marker": { display: "none" },
});
