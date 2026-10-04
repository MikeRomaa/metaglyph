# Metaglyph — Visual editor plan (plan 5: editor projection layer + web app)

## Context

The CLI (`mg check/build/fmt/svg/lsp`) and the Zed extension work. Spec §16 names a fifth workstream, the **editor projection layer**, and spec §9 sketches its tools. This plan turns that sketch into a buildable design for a **Vite + React web app**. The app manipulates `.mg` source through structured edits on the syntax tree. The DSL stays canonical: every canvas action is a text patch, visible in a synced code pane.

Styling and layout are out of scope; a Claude Design session will take this plan as input. The feature section therefore describes **views, states and interactions**, not appearance.

Companion to `2-specification.md` (§9, §16 plan 5) and `3-rust-impl.md`.

### Decisions taken

| Area | Decision |
|---|---|
| Engine | Rust → WASM. New crate `crates/mg-wasm` wraps mg-syntax / mg-hir / mg-eval / mg-font, running in a Web Worker. |
| Storage | Browser only (IndexedDB). Import and export `.mg`; no filesystem or git link. |
| Project shape | One `.mg` file per project. |
| Code pane | Synced CodeMirror 6 pane. Typing re-renders the canvas; canvas edits show up as text changes. |
| Undo | One text history (CodeMirror's). Every canvas edit is a transaction. A drag or slider gesture coalesces into one entry. |
| Invalid source | The canvas shows the last good state plus diagnostics, and is read-only until the text parses. |
| Drag model | **Local inverse drag**: only literals inside the current glyph (body + config) are driven. Top-level lets, params and metrics are never changed by a point drag. |
| Driver choice | The innermost local driver is picked automatically; a key cycles to the next one. |
| Literal precision | Match the rewritten literal's existing decimal count. New raw lengths are whole units; new angles are `deg` with 1 decimal place. |
| New points | Always a named `let` with **raw-unit** coordinates. The name is prompted inline, with the placeholder accepted on Enter. |
| Snapping | Sets position only; it never writes a reference. Relationships come from the relationship tools. |
| Formatting | **Minimal splice**: only changed tokens are replaced, and inserted declarations copy their siblings' indentation. The formatter never runs implicitly. |
| Deleting a referenced declaration | Delete it; the unresolved-name diagnostics show what broke. |
| Relationship tools (v1) | Coincident, intersection, project, fraction, polar, mirror. Deferred: promote-to-param, make-smooth. |
| Spacing and kern drags | **Add a constant** to the field's expression, and update that trailing constant on later drags. |
| Kern units | Per-kern raw/em toggle; new kerns start raw. |
| Params | Sliders write to the source, debounced (~400 ms) into one history entry. |
| Preview | Canvas-drawn from evaluated outlines only (no FontFace/TTF round trip). |
| Library | Project list only. |
| v1 views | Glyph grid, glyph editor, metrics editor, kerning editor, text preview. |
| Glyph editor extras | Components. Deferred: anchors UI, glyph-set UI, dependency inspector. |
| New glyph | Codepoint picker (name derived and editable, `rsb: 0`) plus batch-add of a Unicode block. |

### Spec deltas this plan implies

- **§9.2 Inverse drag** changes to local drivers only, with the innermost driver chosen automatically instead of a picker prompt. Update the spec text when this plan is adopted.
- **§9.1**: advance, `lsb`, `rsb`, metric `y` and kern `by` edits are add-constant rewrites, not inverse solves.

### Prerequisite

The spec now defines glyph `lsb`/`rsb` and `GlyphPlacement` (spec §5.6, §12.1; `plans/3-rust-impl.md` diff), but the crates still require `advance` (`crates/mg-hir/src/schema.rs:168`, `crates/mg-eval/src/graph.rs:416`). **E0 implements that in the crates first**; the metrics editor depends on it.

---

## Part 1 — How the editor manipulates the syntax tree

### 1.1 Architecture of an edit

```
UI gesture ──► worker: mg-wasm edit op(stable NodeId, args)
                 └─ reads current CST, computes TextEdit[] (range, replacement)
          ◄── TextEdit[] ──  main thread dispatches one CodeMirror transaction
                              (userEvent "mg.<op>", history-joined for drags)
CodeMirror doc change ──► worker: reparse + incremental re-eval ──► scene JSON ──► canvas
```

- **All edit logic lives in Rust**, on the rowan CST, in a new module `crates/mg-syntax/src/edit.rs` plus op-level code in `mg-wasm`. The TS side never builds `.mg` text.
- **Targets are stable `NodeId`s** (`crates/mg-syntax/src/stable_id.rs`), so a selection survives a reparse. Literal targets add the token's ordinal within its node.
- **Every edit is text-in, text-out.** The worker returns `TextEdit[]` against the document version it was computed from. The main thread rejects a stale result (the version doesn't match) and asks again.
- **Invalid source:** if the current text has parse errors, edit ops return `Err(ReadOnly)`, and the canvas renders the last good scene with a banner.

### 1.2 Splice rules (minimal splice)

Primitives in `mg-syntax/src/edit.rs`:

| Primitive | Rule |
|---|---|
| `replace_expr(node, text)` | Replace exactly the expression's trimmed range (`mg_syntax::trimmed_range`). |
| `replace_literal(token, value)` | Rewrite one numeric literal. Keep its unit suffix (`deg`, `em`) and its decimal count. Keep a leading unary `-` in place; flip the sign by adding or removing it. |
| `add_constant(expr, delta)` | If the expression is a bare literal, `replace_literal`. If it ends with a top-level `+ <lit>` or `- <lit>` added by an earlier edit, update that literal (drop it when it reaches 0). Otherwise append ` + <delta>` (or ` - <|delta|>`), parenthesizing the original only if its top operator binds looser than `+` (comparison, `and`, `or`). |
| `set_field(config, name, text)` | Replace the value if the field exists. Otherwise append it: on a single-line config, `, name: text` before `)`; on a multi-line config, a new line aligned to the previous field's column (matches the `path lower_bowl` / `arc` style in `samples/a22x-mono.mg`). An empty `()` becomes `(name: text)`. |
| `remove_field(config, name)` | Remove the field, one adjacent comma, and the whitespace between them. If it was on its own line, remove that line. |
| `insert_decl(body, after, text)` | Insert on a new line after `after` (or at body start), copying `after`'s leading whitespace. For a `let` in a glyph body: after the last existing `let`; with no lets, first in the body followed by one blank line. |
| `insert_top_level(kind, text)` | After the last declaration of the same kind (`metric`, `group`, `kern`, `glyph`, `param`, `instance`); with none, at the end of the file. Glyphs are separated by one blank line. |
| `remove_decl(node)` | Remove the declaration and its line; collapse a resulting double blank line to one. |
| `rename(decl, new)` | Rename the declaration and every reference to it in its namespace. Reuse the reference index in `crates/mg-lsp/src/index.rs` (move the shared part into mg-hir if needed). |

Invariants, checked by tests on every op:
1. Bytes outside the edited ranges are unchanged.
2. The result parses with no new syntax errors.
3. Where the op claims to preserve geometry, re-evaluation gives the same values.

### 1.3 Naming

- A new named declaration gets a placeholder (`p0`, `p1`, …; `path0`; `l0` for lines; `e0` for ellipses; `d0` for measurements; `seg0`) that is unique in its namespace. The canvas immediately opens an inline rename field; **Enter keeps the placeholder**, and typing issues a `rename`.
- Control points of new curve segments are auto-named `<to-name>_c1`, `<to-name>_c2`, `<to-name>_c` (for a quad) without a prompt, to keep curve creation quick.
- Names are validated against reserved words (spec §5.4), shadowing (§5.11 rule 3), and duplicates before the rename is applied.

### 1.4 Glyph-editor actions → syntax

Coordinates are raw units unless stated. `pN` means a new named `let`.

**Construction**

| Action | Generates | Removes / changes |
|---|---|---|
| Place construction point | `let pN = (x, y);` | — |
| Line through two points | `let lN = lineThrough(a, b);` | — |
| Horizontal / vertical guide | `let lN = hline(y);` / `vline(x);` | — |
| Line at angle through a point | `let lN = lineAt(p, θdeg);` | — |
| Ellipse / circle by centre and corner | `let eN = ellipse(c, rx, ry);` / `circle(c, r);` (shift-drag) | — |
| Measurement | `let dN = length(b - a);`, drawn as a dimension | — |
| Metric guide | `metric name (y: <n>)` via `insert_top_level` | — |

**Paths and segments**

| Action | Generates | Removes / changes |
|---|---|---|
| Path tool, first click | `let pN = (x, y);` + `path pathN (<config>) {\n  start (at: pN)\n}`. The config copies `stroke`/`caps`/`joins` from the path most recently edited in this glyph; with none, the path starts as a construction path (empty config). | — |
| Path tool, next click | `let pN = (x, y);` + `line (to: pN)` after the selected segment (or last, before `close`) | — |
| Click the start point | `close` | — |
| Click-drag while placing | `cube (c1: pN_c1, c2: pN_c2, to: pN)` with both control lets. `c1` is omitted when the previous segment is a `cube` (smooth by reflection, spec §6.3). | — |
| Segment kind → `line` | — | `remove_field` for `c`/`c1`/`c2`/`center`/`rx`/`ry`/`sweep`/`large`. A control-point `let` left with no references is removed too. |
| Segment kind → `quad` | `c: pN_c`, seeded at the chord's midpoint, or the average of the cube's controls | fields `quad` doesn't admit |
| Segment kind → `cube` | `c1`, `c2` at ⅓ and ⅔ of the chord (quad: exact degree elevation) | fields `cube` doesn't admit |
| Segment kind → `arc` | centre mode: `center: pN_ctr` at the chord midpoint, `sweep: "ccw"` | fields `arc` doesn't admit |
| Drag a radius handle (centre-mode arc) | `rx: <n>, ry: <n>` from the current ellipse, plus `large` if needed | `center` field |
| Toggle sweep / large | `set_field sweep / large` | — |
| Stroke tool / stroke-edge drag | `set_field stroke`; the drag uses local inverse drag on the `stroke` expression | — |
| Fill on / off | `fill: true` + `close` if absent / `remove_field fill` (keeps `close`) | — |
| Caps | `caps: "round"`; per end: `caps: ("round", "butt")` | — |
| Path-wide join | `joins: "bevel"` | — |
| Per-segment join | `joinAt: { segName: "round" }`. An unnamed segment is named first (inline prompt). | — |
| End direction | the adjacent control's `let` becomes `polar(endpoint, len, θdeg)` | the control's previous expression |
| Delete segment | `remove_decl` of the segment; its `to`/control lets are removed if nothing else references them | — |
| Delete let / path | `remove_decl`; dangling references become errors, as intended | — |

**Relationship tools** (spec §9.1). Each rewrites **one local `let`'s expression**. A tool is disabled, with the reason shown, when the target is top-level or when the rewrite would create a cycle (checked against `mg_eval::Graph` before the edit).

| Tool | Rewrites the target `let` as | New literals |
|---|---|---|
| Make coincident | `b` | — |
| Snap to intersection | `meet(l1, l2)` when both lines are named lets; else `meet(lineThrough(a, b), lineThrough(c, d))` | — |
| Project onto line | `project(q, l)` | — |
| Cast onto ellipse | `cast(l, e)` when a named line is picked; else `cast(lineAt(e.center, θdeg), e)`, with `θ` the current position's angle about the centre | `θ` 1 decimal |
| Place at fraction | `mediate(a, b, t)`, with `t` from the current position projected onto `ab` | `t`, 3 decimals |
| Polar from point | `polar(q, len, θdeg)`, from the current position | `len` whole units, `θ` 1 decimal |
| Mirror across axis | `mirror(q, axis)` | — |

**Components**

| Action | Generates | Removes / changes |
|---|---|---|
| Place component | `component (glyph: X, offset: (dx, dy))` | — |
| Drag component | local inverse drag of the offset literals (or of `translate` args in a transform) | — |
| Edit transform (inspector) | `transform: (scale(…), rotate(…deg), translate(dx, dy))` | the `offset` field, whose value moves into `translate` |

### 1.5 Local inverse drag

Applies to point lets, control points, arc centres, stroke edges, and component offsets.

1. **Driver discovery (Rust).** For the dragged value and each axis, collect the **local numeric literals** upstream of it: literals in its own expression, then recursively in local `let`s it references (glyph body + glyph config), stopping at top-level names. This needs literal-level provenance, which `mg_eval::Graph` doesn't have (its nodes are bindings). Add a CST walk in mg-wasm that maps each local `let` to its literal tokens, and use `Graph.deps` to follow local edges.
2. **Innermost driver.** Walk those literals in order: the value's own expression first (left to right), then referenced local lets, breadth-first. Take the **first literal with non-zero sensitivity** in the drag direction; sensitivity is a finite-difference probe via partial evaluation (step 4). A key cycles to the next driver with non-zero sensitivity for the rest of the drag. The choice is remembered per point for the session.
3. **Axes.** A literal pair `(x, y)`, or any expression whose x and y have different drivers (e.g. `(0.514 * w, 0.266 * h)`), drags both axes independently. A single driver that moves both axes (e.g. the angle in `polar`) drags along its track, and the canvas draws the track for the whole drag.
4. **Solve.** Add `mg_eval::evaluate_subgraph(hir, instance, target, overrides: &[(LiteralRef, f64)])`, which evaluates only the target's upstream closure, with no stroking. First try the **linear fast path**: probe at 3 values; if the value is affine in the literal, solve in closed form. Otherwise run a bracketed 1-D minimization (golden section, then secant refinement) of the distance to the pointer, starting at the current value and limited to ±2× the value's magnitude, so it stays continuous with the start.
5. **Axis with no local driver** (e.g. apex y = `h`): that axis is locked. The cursor and a tooltip say why, naming the top-level driver.
6. **Commit.** `replace_literal` with the decimal count preserved. During a drag, text updates are throttled to one per animation frame and history-joined into one undo entry.

### 1.6 Other editors → syntax

**Metrics editor: edge and ink drags.** The editor draws the origin guide at `x = −shift` and the advance guide at `x = advance − shift` (spec §12.1). Only the selected glyph's guides and ink drag; clicking another glyph selects it. Dragging the origin guide changes `lsb` and keeps `rsb` (the advance grows); dragging the advance guide changes `rsb` and keeps `lsb`; dragging the ink shifts it within the advance (`lsb` up, `rsb` down). Each declared field add-constants its own change. When the declared fields can't express the change, one more is declared at its new value: `advance` or `rsb` alone need `lsb` to move the ink, and `lsb` alone needs `rsb` to change the bearings apart.

| Declared | Origin guide drag | Advance guide drag | Ink drag |
|---|---|---|---|
| `advance` | `advance` += d, add `lsb` | `advance` += d | add `lsb` |
| `rsb` | add `lsb` | `rsb` += d | `rsb` −= d, add `lsb` |
| `lsb` | `lsb` += d, add `rsb` | add `rsb` | `lsb` += d, add `rsb` |
| `lsb`, `rsb` | `lsb` += d | `rsb` += d | `lsb` += d, `rsb` −= d |
| `advance`, `lsb` | `advance`, `lsb` += d | `advance` += d | `lsb` += d |
| `advance`, `rsb` | `advance` += d | `advance`, `rsb` += d | `rsb` −= d |

Numeric fields in the inspector take typed values: a bare literal is replaced, anything else is add-constanted.

**Metric lines** (`metric … y:` / `overshoot:`), dragged in the metrics editor: add-constant on `y` or `overshoot`. They are locked in the glyph editor.

**Kerning.**

| Action | Generates | Removes / changes |
|---|---|---|
| New pair | `kern (left: A, right: V, by: -15)` via `insert_top_level` | — |
| Nudge or drag a pair | add-constant on `by` | — |
| Unit toggle | the bare literal, trailing added constant, or literal factor (`-0.05em * k`) converts `-15` ↔ `-0.015em` (÷ `font.em`), keeping its precision: going to `em` adds the decimals the em takes (3 for 1000), coming back removes them | — |
| Kern at group level | `left:` / `right:` set to the group name | — |
| New group | `group name (glyphs: [ a, b ])` | — |
| Add / remove group member | list element insert / remove, comma-aware | — |
| Delete pair | `remove_decl` | — |

The spec §12.2 rules are enforced before the edit: a glyph may be in at most one left-used and one right-used group, and duplicate pairs are refused.

**Params and instances.** A slider writes to the active instance's override field if it has one, and otherwise to the param's `default`. A bare literal is replaced; otherwise the edit add-constants. Writes are debounced ~400 ms, with every write in the gesture history-joined. The slider is clamped to the param's `range`.

**Font info form.** `set_field` / `remove_field` on the `font (…)` directive.

**New glyph.** `glyph <name> (codepoint: '<c>', advance: <a>) {\n}` via `insert_top_level`, where `<a>` is the font's most common `advance` declaration (first on a tie; half an em when no glyph declares one). Not `rsb: 0`: `rsb` and `lsb` measure from the ink, and an empty glyph has none (MG0608), which would also block export; the same goes for an `advance` reading `glyph.*`. Printable characters use a char literal, like `samples/a22x-mono.mg`; other characters use `U+XXXX`. The name comes from the AGLFN (bundled), with `uniXXXX` as the fallback, and is editable before insert. Batch add from a Unicode block (bundled block table) inserts one glyph per missing codepoint in a single transaction.

---

## Part 2 — Application features

All views share one worker-evaluated state for the **active instance**.

### 2.1 App shell and library
- **Library screen:** the project list from IndexedDB (name, updated time, glyph count, error count). Actions: new (empty skeleton: `font`, the five required metrics, `instance Regular ()`), import `.mg` (file picker + drag-drop), open, rename, duplicate, delete (with confirmation), export `.mg`.
- **Workspace:** a view switcher (Grid / Glyph / Metrics / Kerning / Preview), the instance switcher, a diagnostics count, and a "Build TTF" action.
- **Autosave:** the doc is written to IndexedDB, debounced ~1 s. A "saved" indicator is shown.
- **Export:** `.mg` download; TTF per instance via `mg_font::build_fonts` in WASM (a zip when there are several instances), blocked on errors per spec §4.6.

### 2.2 Code pane (all views)
- CodeMirror 6 on the whole file; switching glyphs scrolls to and highlights that glyph's declaration.
- Highlighting from `editors/tree-sitter` via web-tree-sitter, reusing `editors/zed/languages/metaglyph/highlights.scm`.
- Diagnostics from the WASM check (mg-diag spans → `@codemirror/lint`).
- Selecting a canvas object highlights its source range, and moving the cursor into a declaration selects it on the canvas.
- A canvas edit briefly flashes the changed text ranges.
- Deferred: hover and completion (the mg-lsp logic can be exposed later).

### 2.3 Glyph grid
- Tiles in declaration order (the spec §10.6 glyph order), each rendered from the evaluated outline. Each tile shows its name and codepoint, plus badges for errors, "empty" (no ink), and components.
- Filter by name or codepoint, and a toggle for "errors only".
- "+" opens the new-glyph dialog (single codepoint or Unicode block).
- Double-click opens the glyph editor.

### 2.4 Glyph editor
- **Canvas layers** (each can be toggled): metric lines; origin and advance guides; construction points, lines, ellipses and measurements; skeletons with segment endpoints and control handles; the stroked/filled outline; components (dimmed, clickable to open); diagnostics anchored at their geometry (e.g. a curvature-limit interval).
- **Tools:** select/drag, path tool, construction point/line/guide/measurement, component place, plus the relationship tools on the current selection.
- **Inspector:** the selected declaration's fields, as editable expressions (typed text is spliced as-is) and quick controls (stroke, caps, joins, fill, sweep/large, segment kind).
- **Drag feedback:** the active driver literal (and its value) is shown near the cursor, locked axes are indicated, a single-driver track is drawn, and the key hint for cycling drivers is shown.
- **Canvas settings (not in source):** grid and grid snap, snap to points/lines/metrics, zoom and pan.
- **Navigation:** previous/next glyph, and jump to a glyph by name.

### 2.5 Metrics editor
- A row of glyphs (a user-typed string, or a range of the glyph set) with ink bounds, origin/advance guides and the lsb/advance/rsb numbers.
- Guide drag per the §1.6 table, and an inspector for typing values.
- Metric lines can be dragged here (add-constant).
- The font-info form lives here too.

### 2.6 Kerning editor
- Pair list: left, right, value, unit, and level (glyph/group), with sort and filter. The effective value shows the §12.2 precedence (a glyph pair overrides a group pair).
- Live pair preview in context (a user sample string, e.g. `HH<pair>HH` / `nn<pair>nn`), with drag or arrow-key nudges.
- Group panel: create, rename, add/remove members; violations of the one-left/one-right rule are shown before the edit.
- Per-pair raw/em toggle.

### 2.7 Text preview
- Type sample text and see it set with evaluated outlines on a canvas. Kerning is applied from the evaluated kern table (pair lookup, with glyph-over-group precedence), components are decomposed, and instance `slant` is applied as a shear.
- Instance comparison: the same text in every instance, stacked.
- Size and line-height controls.
- It is not guaranteed to match the compiled font's rasterization. "Build TTF" is the check for that.

### 2.8 Params panel (workspace-wide)
- The instance switcher (the active instance drives every view).
- One slider per `param` (clamped to its `range`), with debounced writes per §1.6.
- Instance fields (`slant`, `weightClass`, …) are editable in the inspector.

---

## Part 3 — Implementation

### Layout
- `crates/mg-wasm/` — wasm-bindgen API; built with `wasm-pack build --target web`.
- `crates/mg-syntax/src/edit.rs` — the splice primitives from §1.2.
- `crates/mg-eval` — adds `evaluate_subgraph` with literal overrides (§1.5).
- `editors/web/` — Vite + React + TypeScript. The worker is wrapped with Comlink; the code pane is CodeMirror 6; the canvas uses the 2D Canvas API; storage uses `idb`.

### mg-wasm API (the worker boundary; JSON via serde)
- `open(source) -> DocState` and `update(source, version) -> DocState` (reparse and incremental re-eval via `mg_eval::reevaluate` / `dirty_closure`).
- `diagnostics()`, `glyph_list()`, `glyph_scene(name, instance)`. A scene holds skeleton segments with endpoints and controls, construction values, outline contours (from `mg_eval::glyph_outline`), placement (shift/advance), metrics, and components, each tagged with `NodeId` + text range.
- `kern_table(instance)`, `params()`, `instances()`.
- `edit(op, args, version) -> Result<TextEdit[], EditError>` for every op in Part 1.
- `drag_begin(target, axis_mask)`, `drag_to(pointer) -> TextEdit[]`, `drag_cycle_driver()`, `drag_end()`.
- `build_ttf(instance) -> Uint8Array`.

### Milestones (one developer)

| # | Milestone | Estimate |
|---|---|---|
| E0 | Crates: `lsb`/`rsb` + `GlyphPlacement` per the updated spec | 2–3 d |
| E1 | mg-wasm scaffold, worker, Vite shell, library + IndexedDB, code pane with highlighting and diagnostics | 5 d |
| E2 | Read-only rendering: glyph editor canvas layers, glyph grid, text preview | 5 d |
| E3 | `edit.rs` primitives, TextEdit protocol, history coalescing, rename, read-only-on-error | 5 d |
| E4 | Glyph editor tools: construction, path tool, segment kinds, arc handles, stroke/fill/caps/joins, end direction, components | 8 d |
| E5 | Local inverse drag + relationship tools | 8 d |
| E6 | Metrics editor, params panel, font-info form | 4 d |
| E7 | Kerning editor + groups | 5 d |
| E8 | New-glyph flows (AGLFN, Unicode blocks), TTF export, polish | 3 d |

About 9–10 weeks in total. E0–E3 gate everything; E5 carries the most risk.

---

## Verification

- **Edit ops (Rust):** `cargo test -p mg-syntax -p mg-wasm`. Every op in Part 1 has an insta snapshot fixture (source before → op → source after), run against `samples/a22x-mono.mg` and `samples/metaglyph-sans.mg`. A proptest checks the §1.2 invariants: bytes outside the edit ranges are unchanged, no new parse errors, and geometry is unchanged for preserving ops (rename, coincident with an equal value).
- **Drag solver:** unit tests on the sample glyphs: dragging `a`'s `arc0` moves only `37deg`; apex y of `A` is locked; `0.514 * w` takes the linear path and hits the pointer exactly.
- **Parity:** `build_ttf` from WASM is byte-identical to `mg build --timestamp 0` for both samples.
- **Web:** `npm test` (Vitest) for the TS state and history coalescing. A Playwright e2e test imports `a22x-mono.mg`, drags a point, checks the code pane text and a single undo entry, undoes, and checks the text is byte-identical to the original.
- **Manual:** `npm run dev` in `editors/web`, import a sample, then create a glyph and draw, stroke, kern, adjust spacing, build and install the TTF.
