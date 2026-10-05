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
        // Room for the quick-navigation strip (`nav.ts`).
        marginRight: "1.25rem",
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
    ".cm-tooltip.cm-tooltip-autocomplete > ul": {
        fontFamily: "var(--font-mono)",
        fontSize: "0.75rem",
        maxHeight: "14rem",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
        padding: "0.0625rem 0.5rem 0.0625rem 0.25rem",
    },
    "&.cm-editor .cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]":
        {
            background: "var(--inv)",
            color: "var(--oninv)",
        },
    "&.cm-editor .cm-tooltip-autocomplete li[aria-selected] .cm-completionMatchedText":
        { color: "inherit", textDecoration: "underline" },
    "&.cm-editor .cm-tooltip-autocomplete li[aria-selected] .cm-completionDetail":
        {
            color: "inherit",
            opacity: "0.7",
        },
    ".cm-completionMatchedText": {
        textDecoration: "none",
        fontWeight: "600",
        color: "var(--acc)",
    },
    ".cm-completionDetail": {
        marginLeft: "0.75rem",
        fontStyle: "normal",
        color: "var(--faint)",
    },
    ".cm-tooltip.cm-completionInfo": {
        fontFamily: "var(--font-mono)",
        fontSize: "0.6875rem",
        whiteSpace: "pre-wrap",
        padding: "0.25rem 0.5rem",
        maxWidth: "24rem",
    },
    // Search: the find/replace panel and its matches.
    ".cm-panels": {
        background: "var(--panel)",
        color: "var(--ink)",
        borderColor: "var(--ink)",
    },
    ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--ink)" },
    ".cm-search": {
        fontFamily: "var(--font-mono)",
        fontSize: "0.6875rem",
        padding: "0.25rem 0.5rem",
    },
    ".cm-search input, .cm-search button, .cm-search label": {
        fontFamily: "inherit",
        fontSize: "inherit",
    },
    ".cm-search .cm-textfield": {
        background: "var(--panel2)",
        color: "var(--ink)",
        border: "1px solid var(--rule)",
        borderRadius: "0",
    },
    ".cm-search .cm-button": {
        backgroundImage: "none",
        background: "var(--panel2)",
        color: "var(--ink)",
        border: "1px solid var(--ink)",
        borderRadius: "0",
    },
    ".cm-searchMatch": {
        background: "var(--tok)",
        outline: "1px solid var(--acc)",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
        background: "var(--acc)",
        color: "var(--onacc)",
    },
    ".cm-selectionMatch": { background: "var(--wash)" },
    // Quick navigation beside the scrollbar (`nav.ts`).
    ".cm-mg-nav": {
        position: "absolute",
        top: "0",
        right: "0",
        bottom: "0",
        width: "1.25rem",
        background: "var(--panel)",
        borderLeft: "1px solid var(--rule)",
    },
    // Each tick is clickable across the strip; its mark is the 5px bar at
    // its left edge, centred on the line it points to.
    ".cm-mg-nav-tick": {
        position: "absolute",
        left: "0",
        right: "0",
        height: "9px",
        marginTop: "-4px",
        padding: "0",
        border: "none",
        cursor: "pointer",
        background:
            "linear-gradient(var(--tick), var(--tick)) 1px 3px / 5px 3px no-repeat",
    },
    ".cm-mg-nav-glyph": { "--tick": "var(--con)" },
    ".cm-mg-nav-warning": { "--tick": "var(--acc)" },
    ".cm-mg-nav-error": { "--tick": "var(--err)" },
    ".cm-mg-nav-tick:hover": { backgroundColor: "var(--hi)" },
    // A glyph's character, beside its mark.
    ".cm-mg-nav-label": {
        position: "absolute",
        left: "7px",
        top: "-1px",
        font: "600 9px/11px var(--font-sample)",
        color: "var(--ink)",
        whiteSpace: "nowrap",
        pointerEvents: "none",
    },
    ".cm-mg-nav-tick:hover .cm-mg-nav-label": { color: "var(--acc)" },
    // The problems list (Ctrl+Shift+M).
    ".cm-panel.cm-panel-lint": {
        fontFamily: "var(--font-mono)",
        fontSize: "0.6875rem",
        maxHeight: "11rem",
    },
    ".cm-panel.cm-panel-lint ul [aria-selected]": {
        background: "var(--inv)",
        color: "var(--oninv)",
    },
    ".cm-panel.cm-panel-lint button[name=close]": { color: "var(--mid)" },
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
