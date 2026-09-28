# Metaglyph — Zed extension plan (syntax highlighting + language server)

## Context

`plans/2-specification.md` is the normative language reference; `plans/3-rust-impl.md` builds the compiler this plan reuses. This plan adds editor support for `.mg` files in Zed: syntax highlighting and structural editing from a Tree-sitter grammar, plus diagnostics, navigation, hover, completion, and formatting from a language server.

This is not spec plan 5 (the editor projection layer, spec §9). There is no canvas, no inverse drag, and no structured edits here. It is a text editor for the canonical DSL, which spec §2 makes the source of truth anyway.

**Outcome:** opening `samples/metaglyph-sans.mg` in Zed with the extension installed shows:
- correct highlighting
- an outline of params, metrics, instances, glyphs, and paths
- the compiler's diagnostics as you type
- go-to-definition on every name kind in spec §5.11
- hover showing a binding's type and its evaluated value in each instance
- completion for field names, enum values, functions, and in-scope names
- `mg fmt` on save

### Decisions taken

| Decision | Choice | Consequence |
|---|---|---|
| Highlighting source | **A separate Tree-sitter grammar**, `tree-sitter-metaglyph` | Zed highlights, indents, and builds outlines only from Tree-sitter. The rowan parser (plan 3, M1) stays canonical; the Tree-sitter grammar is a second parser for display only and never feeds the compiler. Drift between the two is caught by a parity test (Z1), not by review. |
| Language server | **`mg lsp`, a subcommand of the existing `mg` binary**, in a new `mg-lsp` crate | One binary to build, release, and version. The server calls `mg-syntax`, `mg-hir`, and `mg-eval` in-process, so its diagnostics are exactly the compiler's. |
| LSP framework | **`lsp-server` + `lsp-types`** (synchronous, from rust-analyzer) | No async runtime. Metaglyph files are small and analysis is fast; a main loop with a debounce and a cancel flag is enough. |
| Text sync | **Full-document sync** | Each change reparses the whole file. Files are a few thousand lines at most, so incremental sync buys nothing yet. |
| Server acquisition | **Settings path → `mg` on `PATH` → GitHub release download**, in that order | Developers get their local build; everyone else gets a pinned release with no setup. |
| Multi-file fonts | **Zed LSP settings list the font's files**; with no setting, each open file is its own font | Spec §5.6 makes a font an ordered file list passed explicitly to the compiler, and the spec has no manifest. The setting mirrors the CLI's file list without adding a language feature. |

---

## Repository layout

```
editors/zed/                     the extension (its own Cargo project, excluded from the root workspace)
  extension.toml
  Cargo.toml                     cdylib, depends only on zed_extension_api
  src/lib.rs                     language server acquisition and settings
  languages/metaglyph/
    config.toml
    highlights.scm  brackets.scm  indents.scm  outline.scm  textobjects.scm
tree-sitter-metaglyph/           the grammar
  grammar.js
  src/                           generated parser.c, committed (Zed builds from it)
  test/corpus/*.txt              parse-tree tests
  test/highlight/*.mg            highlight assertion tests
crates/mg-lsp/                   the server, wired into mg-cli as `mg lsp`
```

The root `Cargo.toml` adds `exclude = ["editors/zed"]`: the extension compiles to WebAssembly with different dependencies and must not join the workspace build.

`extension.toml` points at the grammar in the same repository with `repository`, `rev`, and `path = "tree-sitter-metaglyph"`. During development, `repository = "file:///…"` avoids pushing a commit for every grammar change.

---

## Milestones

Estimates assume one developer who knows the spec. **Z-milestones need only the spec and can start today.** L-milestones depend on the plan 3 milestone named on each. Total **3–4 weeks**.

### Z0 — Extension scaffold (half a day)

- `extension.toml`: `id = "metaglyph"`, name, version, `schema_version = 1`, authors, description, repository, and a license file (the Zed extension registry requires one).
- `languages/metaglyph/config.toml`:
  - `name = "Metaglyph"`, `grammar = "metaglyph"`, `path_suffixes = ["mg"]`
  - `line_comments = ["// "]`, `tab_size = 2` (the Appendix A convention)
  - bracket pairs `()` `{}` `[]` `""` auto-closing; `''` auto-closing only outside strings and comments
- Install via **zed: install dev extension**; confirm `.mg` files get the language in the status bar.
- Pin the `zed_extension_api` version and record it. Check the docs for that version's trait signatures before Z3.

### Z1 — Tree-sitter grammar (3–4 days)

The grammar follows spec §5.1–§5.2 and §5.8, but only as far as highlighting needs: it parses every valid program, and it does not have to reject every invalid one. Field schemas, types, and name resolution stay in the compiler.

- **Lexical (spec §5.1):**
  - `//` comments as `extras`
  - numbers, plus a suffixed number as `number` followed by `token.immediate(choice("deg", "rad", "em", "%"))`, so the suffix is its own node and highlights separately; `2 deg` with a space does not match
  - hex `0x…`, codepoint `U+…`, and character `'…'` literals as distinct nodes, with character escapes as child nodes. Range and scalar-count checks are left to the compiler.
  - strings with escape child nodes
- **Keywords.** Set `word: $ => $.identifier` for keyword extraction. The sharp edges are the words that are both declaration keywords and something else:
  - `font`, `glyph`, `instance` are also namespace roots (spec §5.4): `font (…)` at top level vs `font.em` in an expression
  - `glyph` is also a field key: `component (glyph: six)`
  - `line` is a segment keyword; its constructor is `lineThrough`, so `line` never appears in an expression

  Make the config-field key rule accept these keywords explicitly (aliased to `field_name`), and put the namespace roots in the expression grammar. A corpus test covers each case.
- **Structure (spec §5.2):**
  - `let_statement`: `let name = expr ;`
  - `declaration`: `kind name? config? body?`, with `kind` a field over the 17 declaration keywords. One generic rule, not one per kind, so the grammar never lags a spec change to a field list.
  - `config`: `( field (, field)* ,? )`; a `field` is `name : value`
  - `body`: `{ (let_statement | declaration)* }`
  - `range`: `bound .. bound`, with an optional leading `-` on each bound
- **Expressions (spec §5.8):** the nine precedence levels via `prec.left` / `prec.right`, with `^` right-associative and binding tighter than unary minus. The expression rules are:
  - `call`, `member`, `parenthesized`
  - `tuple` (two or more elements), `list` (`[…]`), `map` (`{…}` in expression position)
  - `namespace` (the five roots), `constant` (`up` `down` `left` `right` `identity`), `boolean`
- **Tests:**
  - `test/corpus/` has a case for every token class, every precedence level, and each keyword edge case above
  - `tree-sitter parse samples/*.mg --quiet` produces no `ERROR` or `MISSING` node
- **Parity test with the rowan parser.** A script runs both parsers over `samples/` and plan 3's `tests/diagnostics/` corpus:
  - a file with no `MG01xx` (syntax) diagnostic must parse with no `ERROR` node in Tree-sitter
  - the reverse is not required, since Tree-sitter is deliberately looser

  Run it in CI once plan 3 M1 exists. Until then, the sample is the only check.

### Z2 — Queries (1–2 days)

**`highlights.scm`.** Use Zed's standard capture names so every theme colours them.

| Syntax | Capture |
|---|---|
| Declaration keywords, `let` | `@keyword` |
| Segment kinds `start` `line` `quad` `cube` `arc` `close` | `@keyword` (candidate: `@function.builtin`; decide by how the sample reads) |
| `and` `or` `not` | `@keyword.operator` |
| Operators, `..` | `@operator` |
| Declaration names after `glyph` / `group` | `@type` |
| Declaration names after `param` / `metric` | `@constant` |
| Other declaration names (`let`, `path`, `anchor`, segments, `instance`) | `@variable` |
| Config field names | `@property` |
| Call identifiers matching the spec §5.9 list | `@function.builtin` (via `#any-of?`) |
| Other call identifiers | `@function` (an unknown function, which the LSP will flag) |
| `font` `glyph` `glyphs` `instance` `math` in expressions | `@namespace` |
| Members after `.` | `@property` |
| `up` `down` `left` `right` `identity` | `@constant.builtin` |
| `true` `false` | `@boolean` |
| Numbers, hex, codepoint | `@number` |
| Suffix `deg` `rad` `em` `%` | `@type` (so the unit stands out from the magnitude) |
| Character literal | `@string.special` |
| Strings; escapes | `@string`; `@string.escape` |
| Strings in `caps` `joins` `align` `sweep` fields, and `joinAt` values | `@string.special.symbol` (they are enum values, spec §5.5) |
| Comments | `@comment` |
| `( ) { } [ ]`; `, ; :` | `@punctuation.bracket`; `@punctuation.delimiter` |

The other query files:
- **`brackets.scm`:** the three bracket pairs and string quotes.
- **`indents.scm`:** `@indent` on `body`, `config`, `list`, `map`, and multi-line `call` / `tuple`; `@end` on the closing bracket.
- **`outline.scm`:**
  - top level: `font`, each `param`, `metric`, `let`, `instance`, `group`, `glyph`
  - nested under a glyph: its `path`s and `anchor`s
  - `kern` shows as `kern left → right`, using `@context` for `left` and `@name` for `right`
- **`textobjects.scm`:** a `glyph` declaration as `@class.around`/`@class.inside`, a `path` as `@function.around`/`@function.inside`, and comments as `@comment.around`. This gives vim-mode `]]`-style motion between glyphs.

**Test:** `test/highlight/*.mg` files with `// ^ keyword`-style assertions, run by `tree-sitter test`. Then open the sample in Zed with a light and a dark theme.

**Z2 is a shippable release on its own:** syntax-only, with no server required.

### L0 — Server skeleton and syntax diagnostics (2 days) · needs plan 3 M1

- `mg lsp` speaks JSON-RPC over stdio. It handles `initialize` / `shutdown` / `exit` and `didOpen` / `didChange` / `didClose`, and keeps a document store keyed by URI.
- **Position encoding.** Advertise `utf-8` when the client offers it in `general.positionEncodings`; otherwise use UTF-16. A per-document line index converts rowan byte offsets either way. Character literals and comments carry non-ASCII text, so test with `'é'` and a `// ══` rule line.
- **Diagnostic mapping from `mg-diag`**, a single function used by every stage:
  - `code` → `Diagnostic.code`, with `codeDescription.href` pointing at the error-code table
  - primary label → `range` and `message`
  - secondary labels → `relatedInformation`
  - `help` and `note` → appended to the message on separate lines (L5 turns `help` into code actions)
  - `Severity::Warning` → `DiagnosticSeverity::WARNING`
- On every change: reparse, then publish syntax diagnostics immediately.

### L1 — Static analysis, symbols, navigation (3 days) · needs plan 3 M2

- **Stage-two diagnostics:** HIR lowering, field validation, name resolution, type checking, and path structure (spec §13 classes 1–4 and "path structure"). Published with the syntax diagnostics in one `publishDiagnostics` call per file.
- **Font assembly.**
  - With the `fonts` setting (below), each listed font's files are analysed together, and a diagnostic is published to the file it points into.
  - Without the setting, a file is its own font. A lone glyph file will report the missing `font` directive and reserved metrics; that is correct under the spec, and the setting is how you avoid it.
- **`textDocument/documentSymbol`:** the hierarchy from `outline.scm` (glyph > path > named segment, plus lets and anchors), built from HIR so names are exact.
- **`textDocument/definition`**, for each spec §5.11 namespace:
  - bare identifiers → glyph scope, then top level
  - `glyphs.X` and `glyphref` / `groupref` fields → the glyph or group
  - `glyphs.X.anchorName` → the anchor
  - `glyphset:` values → the first glyph declaring that set
  - `follows:` → the path
  - `joinAt` keys → the named segment
  - across files, when the `fonts` setting lists several
- **`textDocument/references`:** the same resolution, inverted. The HIR already has every use site after resolution.

### L2 — Completion and static hover (3 days) · needs plan 3 M2

The M2 field-schema table is the single source for field names, types, defaults, and enum sets. Completion and hover read that table; they never copy it. A test asserts every schema entry has hover text.

**Completion** is chosen by the cursor's CST context:

| Context | Offers |
|---|---|
| Inside a declaration's `( … )`, at a field name | That kind's fields not yet present, as `name: $0` snippets. Mutual exclusions apply: no `rx` after `center`. |
| Value of `caps` / `joins` / `align` / `sweep` / a `joinAt` entry | The legal strings, quoted |
| Value of a `glyphref` / `groupref` field | Glyph and group names (default set only, spec §5.7) |
| `joinAt` key | The enclosing path's segment names |
| `glyphset:` | Existing glyph-set names |
| `follows:` | Paths with a body in the same glyph |
| Expression position | Glyph-scope names, top-level names, built-in constants, `math`/`font`/`glyph`/`glyphs`/`instance`, and §5.9 functions as call snippets with parameter placeholders |
| After `glyphs.` | Glyph names |
| After `glyphs.X.` | `advance`, `bbox`, and X's anchors |
| After `font.` / `glyph.` / `instance.` / `math.` | Their spec §5.10 members |
| After `.` on a typed value | Members of its type (`rect`, `zone`, `pair`), from the M2 type checker |
| Top level / glyph body / path body | The declaration kinds legal there |

**Hover (static):**
- On a name: its declaration kind, its type, and the declaration line.
- On a function: its §5.9 signature and one-line description.
- On a field name: type, required or default, and legal values.
- On a suffixed number: the converted internal value (`152deg` → `2.6529 rad`).
- On a codepoint or character literal: the decimal value and Unicode name, if the Unicode data is already a dependency by then.

### L3 — Evaluation-backed diagnostics and hover (2–3 days) · needs plan 3 M3, M4 for geometry

- **Evaluation runs off the keystroke path.** After 300 ms without edits, and only when stages one and two report no errors, evaluate every instance.
  - A new edit sets a cancel flag that evaluation checks between graph nodes.
  - Plan 3's memoization and dirty-set (M3) keep re-evaluation local to the edited glyph.
- **Per-instance diagnostics** (cycle, domain, geometry, metrics). An error that fires in several instances is published once, with the instances listed in the message: `curvature radius below stroke/2 … [Bold]`. Cycle diagnostics put every hop in `relatedInformation`.
- **Hover values.** On a `let`, `param`, `metric`, or anchor, show the evaluated value per instance:

  ```
  let hair: num
  Regular 86 · Bold 128 · Condensed 86
  ```

  Pairs show as `(x, y)`, rects as their four edges, and zones as `.y` / `.ink`. When evaluation of that node failed, show the error instead. This is the most useful feature in the plan for a parametric language, since reading `stem * contrast` does not tell you what it is.
- Export-class diagnostics (spec §13 "Export") need a full build and are not run by the server.

### L4 — Formatting (half a day) · needs plan 3 M1 formatter

`textDocument/formatting` returns one whole-document edit from `mg fmt`. When the document has a syntax error it returns no edits: formatting around `ERROR` nodes is partial-text tolerance, which belongs to spec plan 5.

Zed's `format_on_save` then uses it with no extension code.

### Z3 — Server acquisition in the extension (1–2 days) · after L0

`src/lib.rs` implements `zed::Extension`:

- **`language_server_command`:**
  1. If `lsp.metaglyph.binary.path` is set, use it, with its `arguments` or `["lsp"]` by default.
  2. Else if `worktree.which("mg")` finds a binary, use it with `["lsp"]`.
  3. Else find the latest GitHub release of the repo:
     - pick the asset for `zed::current_platform()` (os × arch)
     - download and extract it into the extension's working directory, reporting progress through `set_language_server_installation_status`
     - `make_file_executable`, cache the path, and delete older versions
- **`language_server_workspace_configuration`:** forward `lsp.metaglyph.settings` from the worktree's Zed settings. The server reads `fonts` from it:

  ```json
  "lsp": { "metaglyph": { "settings": {
    "fonts": [["src/metrics.mg", "src/caps.mg", "src/figures.mg"]]
  } } }
  ```

  Each inner list is one font's ordered file list, relative to the worktree (spec §5.6 order).
- Register the server in `extension.toml` under `[language_servers.metaglyph]` with `languages = ["Metaglyph"]`.
- Handle errors explicitly: a failed download with no cached binary returns an error naming the settings key, so the user knows how to point at a local build.
- Do not use `cfg`/`std::env` for platform detection; they do not work in the WebAssembly target.

### R — Release pipeline (1 day) · alongside Z3

- **CI job on tag:** build `mg` for x86_64/aarch64 × Linux/macOS/Windows, and attach assets named `mg-<os>-<arch>.{tar.gz,zip}`. The names must match what Z3 constructs, so define them in one place and test Z3's name function against the list.
- **Publishing:**
  - Open a PR to `zed-industries/extensions` adding the repo as a submodule, with `path = "editors/zed"` in their `extensions.toml`
  - Bump `extension.toml` `version` and the grammar `rev` together on each release
  - Z2 can be published before any L milestone exists; the extension works without a server and just logs that none was found

---

## Sequencing

```
Z0 → Z1 → Z2                         (spec only; ship syntax-only here)
plan 3 M1 → L0 → L4
            L0 → Z3 → R              (ship with server)
plan 3 M2 → L1 → L2
plan 3 M3/M4 → L3
```

- Z1 carries the most design risk: the keyword edge cases, plus drift from the rowan parser.
- L3 carries the most performance risk. If a full-font evaluation per edit is too slow on a real character set, restrict evaluation to the instances of the glyphs in the edited file.

## Deferred

- **Semantic tokens** from the server: distinguish param / metric / let / path uses, which Tree-sitter cannot. Zed supports extension-provided `semantic_token_rules.json`.
- **Rename**: must reject reserved words, shadowing, and collisions per spec §5.11.
- **Code actions from `help`** (L5):
  - near-miss renames
  - adding a missing required field
  - quoting a bare enum value
- **Inlay hints** showing evaluated values after `let` lines.
- **Signature help** for §5.9 calls.
- **Glyph preview.** Zed's extension API has no custom views, so a live outline preview belongs to spec plan 5, not here.

## Verification of this plan's output

Run from Git Bash:

```
cd tree-sitter-metaglyph && tree-sitter generate && tree-sitter test
tree-sitter parse ../samples/*.mg --quiet                     # no ERROR nodes
cargo test -p mg-lsp                                         # stdio round-trip tests: open sample, assert diagnostics/definition/hover/completion
```

Then in Zed:

1. **zed: install dev extension** → select `editors/zed`
2. Open `samples/metaglyph-sans.mg`:
   - highlighting matches the Z2 table
   - the outline panel lists all 16 glyphs
3. Change `range: 20..260` to `range: 200..260` → an error appears on `param stem` (default out of range)
4. Hover `hair` → `Regular 86 · Bold 128 · Condensed 86`
5. Cmd/Ctrl-click `six` in glyph `nine` → jumps to `glyph six`
6. In a `path (…)`, type `joins: "` → completion offers `miter`, `round`, `bevel`
7. Save a misformatted file → it is reformatted
