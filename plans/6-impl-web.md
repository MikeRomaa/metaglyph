# Visual editor — implementation plan (plan 5 × "Metaglyph Hybrid" design)

## Context

`plans/5-visual-editor.md` defines *what* the editor does (structured CST edits, local inverse drag, add-constant spacing/kern edits, mg-web worker). The Claude Design file `Metaglyph Hybrid.dc.html` (project `75596478…`) defines *how it looks*: an engineering-drawing sheet with 4 tabs, a left inspector column, a centre SVG drawing, a right source pane and a title-block status bar. `editors/web/` is a fresh Vite 8 + React 19 + React Compiler scaffold. E0 (`lsb`/`rsb` + placement) is already done (commit `cfc1eb9`).

Decisions taken with the user:
- **Scope = the 4 designed sheets only**: 01 GLYPHS, 02 GLYPH, 03 SPACING, 04 KERNING. Deferred: library screen (single project), text preview, params sliders.
- **SVG** for all drawing (not Canvas 2D as plan 5 said). Theme tokens are CSS vars, so SVG picks them up.
- **CodeMirror 6** for the source pane, themed to the design.

Deviations from plan 5 to record in `plans/5-visual-editor.md` when adopted: SVG rendering; v1 = 4 sheets; glyph grid becomes the charset-coverage sheet; highlighting via a TS `StreamLanguage` (mirrors the mg lexer) instead of web-tree-sitter.

---

## 1. Design system (from the .dc.html)

- Copy the design into `editors/web/design/Metaglyph Hybrid.dc.html` as a reference (not built).
- `src/styles/tokens.css`: the exact `:root` tokens (`--page --bg --panel --panel2 --ink --mid --faint --rule --grid --acc --onacc --con --err --hi --tok --wash --hatch --inv --oninv --str`) plus the dark set under `[data-theme="dark"]`. Add `--hatch-bg` helper = `repeating-linear-gradient(135deg, transparent 0 6px, var(--hatch) 6px 7px)` (used for locked/disabled/missing).
- Fonts: `@fontsource/ibm-plex-mono` (400/500/600), `@fontsource/ibm-plex-sans-condensed` (400–700), `@fontsource/ibm-plex-sans` (400/500) — bundled, works offline.
- Type roles (CSS classes in `src/styles/type.css`): `.label` = Plex Sans Condensed 600 10px, letter-spacing .14em, uppercase, `--mid`; `.mono` = Plex Mono 11px; `.tab` = Condensed 600 11px .1em.
- Primitives in `src/ui/`: `Segmented` (butt/round/square style), `Section` (header row + ln ref), `KvGrid`, `BomTable` (inverted header, numbered balls, highlighted row), `Chip`, `Hatched`, `Modal` (hatched backdrop, 6px hard shadow).
- Theme: `data-theme` on `<html>`, LIGHT/DARK buttons in header, persisted in `localStorage` (try/catch), default from `prefers-color-scheme`.
- Layout: design is 1440×900 fixed; implement fluid — left column 264px, source pane 400px, centre `flex:1`, min app width 1280px. Keep the decorative zone rulers (1–8 / A–E) on the outer frame.

## 2. App structure (`editors/web/src/`)

```
main.tsx, App.tsx               shell: Frame > Header, Body(Left | Sheet | Source), StatusBar
styles/                         tokens.css, type.css, base.css
ui/                             primitives above
shell/Header.tsx                METAGLYPH + file menu (New / Import .mg / Export .mg), tabs 01–04,
                                INSTANCE ▾, LIGHT/DARK, EXPORT TTF
shell/StatusBar.tsx             font name · x · y (⌀ when locked) · SNAP · GRID · hint · DESIGNER · REV · SHEET 0N / 04
shell/Globals.tsx               GLOBALS footer of left column: top-level lets (name, expr, value), read-only
source/SourcePane.tsx           CodeMirror 6; header "SOURCE · file · ln N"; footer "✓ N PROBLEMS" + "undo · <last mg.op>"
source/mgLanguage.ts            StreamLanguage tokenizer + HighlightStyle (kw ink 600, number acc, string str, comment faint, punct faint)
source/sync.ts                  selection↔range highlight (--hi line, --tok token, --acc gutter), edit flash, stale-version guard
sheets/glyphs/                  GlyphsSheet, CharsetList, Legend, CellGrid, CoverageStrip, AddGlyphsModal
sheets/glyph/                   GlyphSheet, GlyphCanvas (+ layers/*), ToolPalette, TitleBlock, DragTip,
                                inspector/{Selection, Drivers, Constraints, PathProps}
sheets/spacing/                 SpacingSheet, SpacingRow (SVG), SpacingBom, SpacingFields, MetricLines, FontInfo
sheets/kerning/                 KerningSheet, KernLines (SVG), NudgeBar, KernBom, Groups, Effective
engine/worker.ts                Comlink-exposed wrapper around the wasm module
engine/client.ts                typed Comlink proxy, versioned requests
engine/types.ts                 DocState, GlyphScene, KernTable… (mirror of mg-web serde types)
state/store.ts                  zustand: doc text+version, active sheet/glyph/instance, selection, tool, layers, theme
state/persist.ts                idb: one "current" doc, autosave debounced 1 s, "saved" flag
svg/                            viewport (y-flip `matrix(1 0 0 -1 0 ascender)`, zoom/pan), path-d from contours, hit-testing
```

Deps to add: `comlink`, `idb`, `zustand`, `codemirror`, `@codemirror/{state,view,language,lint,commands}`, `@fontsource/*`, `vitest`, `@playwright/test`. Scripts: `"wasm": "wasm-pack build ../../crates/mg-web --target web --out-dir ../../editors/web/src/wasm"` (gitignore `src/wasm`), `"dev"` runs `wasm` first. Worker via `new Worker(new URL('./engine/worker.ts', import.meta.url), { type: 'module' })`. Toolchain: `rustup target add wasm32-unknown-unknown` (only `wasm32-wasip2` is installed).

## 3. Engine (Rust) — as plan 5 Part 3, unchanged except scope

- `crates/mg-web` (add to workspace): `open/update(source, version) -> DocState`, `diagnostics`, `glyph_list`, `glyph_scene(name, instance)`, `spacing_row(names, instance)`, `kern_table(instance)`, `top_level_lets(instance)`, `instances`, `font_info`, `edit(op, args, version)`, `drag_begin/drag_to/drag_cycle_driver/drag_end`, `build_ttf(instance)` (`BuildOptions { timestamp }` passed from JS `Date.now()/1000`).
- Reuse: `mg_eval::{evaluate, reevaluate, dirty_closure, glyph_outline, Graph}`, `mg_font::build_fonts`, `mg_syntax::{trimmed_range, stable_id::node_id}`, `mg_lsp::index::Index::{references, resolve}` for rename (move shared part into mg-hir if mg-lsp pulls non-wasm deps like lsp-server).
- New: `crates/mg-syntax/src/edit.rs` (plan 5 §1.2 primitives), `mg_eval::evaluate_subgraph` with literal overrides (§1.5).
- Scene additions the design needs: per-point `drivers[]` (literal text, owning let, sensitivity kind: linear/solved/top-level), per-axis lock + locking top-level name; path props (stroke, caps, joins, fill) with source ranges; construction lines/points labelled with their expression; declared `measurement` lets.

## 4. Sheet → data/edit mapping

**Shell.** INSTANCE ▾ lists `instances()`; active instance drives all sheets. EXPORT TTF → `build_ttf` per instance (zip if >1), disabled with reason when errors. File menu handles New (skeleton per plan 5 §2.1), Import (picker + drop), Export `.mg`.

**01 GLYPHS** (replaces plan 5 glyph grid + new-glyph dialog)
- Left CHARACTER SETS: pseudo-set **"All glyphs"** first (declaration order, includes unencoded glyphs), then bundled Unicode blocks (`src/data/blocks.ts`) showing `in/total` + bar. LEGEND and the AGLFN note as designed.
- Cells: evaluated outline thumbnail (from `glyph_list` + cached outlines), hex, tag (`NEW`, `✓`, and an `ERR` tag in `--err` for glyphs with diagnostics); missing = hatched. Click missing → pick; click in-font → open 02 GLYPH. `⇧`-click picks a range. Coverage strip mirrors cells.
- PICK ALL MISSING / CLEAR / ADD N GLYPHS → `AddGlyphsModal`: AGLFN names (`src/data/aglfn.ts`), editable, validated (identifier, reserved words, duplicates incl. existing glyphs), preview of the first two decls, INSERT ↵ = one `mg.add_glyphs` transaction via `insert_top_level`.

**02 GLYPH**
- Toolbar: VIEW 02-<name>, ‹ prev / current + U+ / next ›, layer toggles METRICS GUIDES CONSTR DIMS SKEL OUTLINE.
- Canvas layers (SVG, glyph units, y-flipped group): grid; metric lines with name + expr; origin/advance guides (`x = −shift`, `advance − shift`) and centre line; outline as wash (`--wash`, real stroked contours from `glyph_outline`, not a fake wide stroke); construction lines/points (`--con`, diamond markers, `name · expr` labels); skeleton (ink) with square on-curve handles, selected = `--acc`; DIMS = declared measurements + auto dims (advance, selected point's driver dimension) + path callouts (`PATH stem`, `STROKE ⌀ 50`); DragTip box (`INVERSE DRAG · stem1.x`, `old → new`, `driver i/n · linear|solved · exact hit`, `⌀ y locked ← h (top-level)`).
- Floating ToolPalette: V select, P path, `.` construction point, L line, G guide, M measure, C component; ACTIVE label. Keyboard shortcuts = the key letters.
- TitleBlock (top-right): font name · GLYPH x, CP, EM, ADV (`expr · value`), REV.
- Left inspector: SELECTION (decl kind + name, expression with active driver literal on `--tok`, X/Y values, locked axis hatched with ⌀); DRIVERS · X/Y (list, ● active, sensitivity, **Tab cycles**, scrub slider = `drag_to` along the driver, lock explanation); CONSTRAINTS 2×3 grid (CO IX PJ FR PL MR → plan 5 relationship tools; disabled = hatched + reason line); PATH · <name> (stroke value, caps / joins segmented, fill) → `set_field`/`remove_field`.

**03 SPACING**
- Toolbar: STRING input (default: current glyph + neighbours, e.g. `AaBb`).
- SVG row: metric lines, one column per glyph (hi-lit active), origin/advance guides draggable per plan 5 §1.6 table, lsb/rsb numbers, advance dimension + name; live callout (`ADD lsb: 15 · origin guide Δ +40` or `advance += d`). Metric lines draggable (add-constant on `y`).
- BOM table: item, glyph, advance (`expr · value`), LSB, RSB, DECLARES (`advance → + lsb`). Row click = active glyph.
- Left: SPACING · <glyph> fields (advance/lsb/rsb: source expr, value; `derived` faint; new field on `--hi`) + explanation; METRIC LINES (name, expr, value, typed edits); FONT INFO form (`set_field` on `font(…)`).

**04 KERNING**
- Toolbar: CONTEXT strings (default `bb<pair>bb · 11<pair>11`, editable).
- Two SVG lines with kern applied (glyph-over-group precedence from `kern_table`), pair in `--acc`, guide pair + kern dimension label. Drag the right glyph or ←/→ (⇧ = 10) = add-constant on `by`.
- NudgeBar: −10 −1 · `L → R value` · +1 +10 · RAW/EM toggle (per plan 5 unit conversion).
- BOM table: item, left, right, by, unit, level; `+ NEW PAIR`. Row click selects pair.
- Left: GROUPS (name, LEFT/RIGHT-USED · n pairs, member chips, `+` chip, draft group hatched with §12.2 violation message in `--err`, `+ NEW`); EFFECTIVE · <left> → RIGHT table (glyph, value, why; winning glyph pair in `--acc`).

**Source pane (all sheets).** Selected declaration: line `--hi`, gutter `--acc`, driver/changed token `--tok`; sheet-relevant decls get an ink gutter (glyph body / kern+group lines / new glyphs). Scroll to selection on sheet/glyph change. Diagnostics via `@codemirror/lint`. Canvas edits dispatch one transaction with `userEvent: "mg.<op>"`; drags/nudges join history; footer shows the last op name.

## 5. Milestones (E0 done)

| # | Work | Est. |
|---|---|---|
| W1 | mg-web scaffold + worker + Comlink; design tokens, frame, header, status bar, theme; CodeMirror pane (highlight, lint); idb autosave; import/export `.mg` | 5 d |
| W2 | Read-only sheets from real scenes: 02 canvas layers + title block + globals, 01 cells/blocks/coverage, 03 row + BOM, 04 lines + BOM; instance switcher; selection↔source sync | 5 d |
| W3 | `edit.rs` primitives + TextEdit protocol, history joining, rename prompt, read-only-on-error banner, edit flash | 5 d |
| W4 | 02 tools (palette, path tool, construction, segment kinds, arcs, PATH inspector, components) | 8 d |
| W5 | Local inverse drag, DRIVERS panel, DragTip, CONSTRAINTS tools | 8 d |
| W6 | 03 guide/metric drags, typed fields, font info | 3 d |
| W7 | 04 nudges/drag, RAW/EM, new pair, groups + rule checks | 5 d |
| W8 | 01 add-glyphs flow (AGLFN, blocks, modal), EXPORT TTF, polish | 3 d |

≈ 8½ weeks. W1–W3 gate the rest; W5 carries the most risk.

## 6. Verification

- Rust: `cargo test -p mg-syntax -p mg-eval -p mg-web` — insta before/after fixtures for every op on both samples; proptest for the §1.2 invariants; drag-solver tests (`stem1` `0.500 * w` linear exact hit; A apex y locked by `h`; `a` `arc0` moves only `37deg`).
- Parity: `build_ttf` from WASM byte-identical to `mg build --timestamp 0`.
- Web: `npm run lint`, `npm run build`, `npx vitest` (store, history joining, AGLFN name validation, StreamLanguage tokens).
- Playwright e2e: import `samples/a22x-mono.mg` → 02 GLYPH A → drag `stem1` → source shows the new literal, one undo entry → undo → text byte-identical. Plus: 01 add 3 missing glyphs = one undo step; 04 nudge −1 updates `by`.
- Visual: `npm run dev`, compare each sheet against the design in light and dark.
