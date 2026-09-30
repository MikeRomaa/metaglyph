# Metaglyph — Rust implementation plan (front end → exporter)

## Context

`plans/2-specification.md` is the normative reference; `plans/1-research.md` holds rationale and rejected alternatives. No code exists yet.

This plan implements four of the five workstreams in spec §16: **DSL surface (1)**, **evaluation engine (2)**, **geometry kernel (3)**, and **font compiler (4)**. The geometry kernel is included because there is no path from source text to a `.ttf` without it. Plan 5 (editor projection layer) is out of scope, but two of its requirements are built in from the start because they are cheap now and expensive later: a lossless CST and a memoizable dependency graph.

**Outcome:** `mg build samples/metaglyph-sans.mg -o out/` produces installable TTFs for the three instances in spec Appendix A, byte-identical across runs on the same target.

**Acceptance scope.** Spec §15's acceptance test covers uppercase, lowercase, digits, and punctuation. This plan's acceptance target is narrower: the 16 glyphs of Appendix A in its three instances. The full character set is the first follow-on project.

### Decisions taken

| Decision | Choice | Consequence |
|---|---|---|
| Output formats | **TTF only** | `write-fonts` covers every table needed. OTF/CFF (and with it all of spec §11.1–§11.3) and WOFF2 are deferred. TrueType output is unhinted with a `gasp` table (spec §11.4). |
| Stroker | **`kurbo::stroke`**, bevel joins, then join splicing | kurbo supplies error-bounded offsets and the SVG cap set per end. It takes one join per stroke, so `joinAt` is implemented by stroking with `Join::Bevel` and rewriting each corner's bevel chord in place (M4). Each path stays one seamless outline, with no overlaid join shapes. The spec's curvature check (§7.2) runs first, as a hard error. |
| Stroker oracle | **`tiny-skia` stroker, test-only** | An independent implementation (a Skia port) for the spec §15.4 differential test. kurbo is never tested against itself. |
| Syntax tree | **rowan CST → typed AST** | Trivia and formatting survive, and §9.3 needs no front-end rewrite later. |
| Diagnostics | **rustc-style, designed at M0** | rowan supplies spans and `ERROR` nodes, not messages. Labeled spans, error codes, and structured `help` are threaded through the parser from the first commit. |
| GPOS | **`write-fonts` GPOS builders** | Pair-positioning tables are built directly from the evaluated kern data, with no feature-file text in between. Confirm the builder API exists in the pinned `write-fonts` version at M0; if not, fall back to generating FEA and compiling with `fea-rs`. |

---

## Workspace layout

Single cargo workspace at the repo root. Crates are listed in dependency order; each depends only on crates above it.

| Crate | Owns | Spec |
|---|---|---|
| `mg-diag` | `Diagnostic`, error codes, labeled spans, `help`/`note`, near-miss suggestions, `codespan-reporting` rendering | §13 |
| `mg-syntax` | Lexer, rowan CST, parser, typed AST layer, formatter | §5.1–5.2 |
| `mg-hir` | CST → HIR lowering, field schemas, enum validation, name resolution, static type checking, structural checks | §5.3–5.8, §5.11 |
| `mg-geom` | Pure geometry on kurbo types, with no dependency on evaluation: segment realization (quad elevation, arc solve in both modes, arc realization), curvature check, stroking with join patches, filled contours, Bézier clipping, contour roles, winding | §6, §7, §8 |
| `mg-eval` | Dependency graph, topological evaluation, cycle reporting, `Value`, construction library. Calls `mg-geom` to realize paths and compute `.bbox` | §4, §5.9–5.10 |
| `mg-font` | Slant, extrema, cu2qu, zone snap and quantization, glyf/cmap/metrics assembly, kerning, instances | §10–§12 |
| `mg-cli` | `build`, `check`, `fmt`, `svg`, `dump-graph` — ships the binary as `mg` (`[[bin]] name = "mg"`) | — |

Source file extension: `.mg`. Conformance sample: `samples/metaglyph-sans.mg`, verbatim from spec Appendix A. `mg` takes an ordered list of source files (spec §5.6).

### Dependencies

`rowan` (CST) · `kurbo` (Bézier math, `stroke`, `CubicBez::approx_spline`, extrema) · `write-fonts` (table assembly, GPOS builders) · `read-fonts`/`skrifa` (test-side verification) · `codespan-reporting` · `indexmap` · `clap` · dev: `insta`, `proptest`, `tiny-skia`. Pin exact versions at M0 and record them in `Cargo.lock`.

Hand-written rather than pulled in: the lexer (rowan needs every trivia token preserved, and the token set in spec §5.1 is small), and the topological sort (it needs declaration-index tie-breaking per spec §14).

**Determinism (spec §14) is a crate-wide rule, not a milestone:**
- No `HashMap` iteration anywhere; `IndexMap`/`BTreeMap` only.
- `f64` throughout.
- Topological ties broken by declaration index.
- `head.created`/`modified` settable to a fixed value.

Output is byte-identical on the same target only: `std` transcendental functions and kurbo's internals use the platform math library, which spec §14 permits.

---

## Milestones

Estimates assume one developer who already knows the spec. Total **6–8 weeks**.

### M0 — Scaffold and the diagnostic model (2 days)
Workspace, seven crates, CLI skeleton with `check` stubbed. CI running `cargo test` + `clippy -D warnings` + `fmt --check`. Pin dependencies and confirm the `write-fonts` GPOS builder API.

**`mg-diag` is designed here, not grown later.** rowan supplies spans and `ERROR` nodes but no messages. Retrofitting labeled spans through a parser that carries one offset means rewriting the parser. The model:

```rust
struct Diagnostic {
    code: Code,                  // MG0102 — stable, greppable, documented
    severity: Severity,          // Error | Warning (spec §13)
    message: String,             // "expected `,` or `)`, found `deg`"
    primary: Label,              // span + the short claim
    secondary: Vec<Label>,       // "unclosed block opened here"
    help: Vec<String>,           // "add a `,` after the field value"
    note: Vec<String>,
}
```

- **One code per error kind**, grouped by spec §13 class (`MG01xx` syntax, `MG02xx` type, …), documented in one table.
- **Secondary labels are mandatory for paired constructs.** An unclosed `{`, `(`, or string labels the opener as well as the point of failure, so the parser carries the opening span down the recursion from the first commit.
- **`help` is a separate field, not prose in the message.** The renderer prints it as rustc's `help:` line, and a later editor can turn it into a quick-fix.
- Rendering via `codespan-reporting`.
- **Corpus snapshot tests from M0 onward:** `tests/diagnostics/*.mg`, each a small broken source with an `insta` snapshot of the rendered output.

### M1 — Lexer, CST, parser (4–6 days)
Parses `samples/metaglyph-sans.mg` with zero diagnostics, and `print(parse(src)) == src` byte-for-byte on the sample and the whole diagnostics corpus.

- **Token set per spec §5.1.** A suffixed number (`deg` `rad` `em` `%`) is one token with the suffix recorded; whitespace between number and suffix makes them two tokens. Also `..`, plus `//` line comments as trivia.
- **Three integer spellings besides decimal**, each one `INT`-valued token that lowers to a plain `num`, with no codepoint type anywhere downstream:
  - hex `0x…`, rejecting values above 2^53
  - `U+` codepoints, rejecting values above `10FFFF`
  - character `'…'`, one Unicode scalar value (UTF-8, not only ASCII) with escapes `\'` `\\` `\n` `\t`; empty, multi-scalar, unterminated, and unknown-escape cases each get their own code, and unterminated labels the opening `'`

  The CST keeps the source spelling, so `mg fmt` never rewrites `'A'` as `65`.
- **One generic block routine** parses `<kind> <name>? ( config )? { body }?`. Kind-specific field rules belong to M2.
- **`range` is a grammar production**, `"-"? number ".." "-"? number` (spec §5.2), so negative bounds parse.
- **`(a, b, …)` emits a single `TUPLE` node** with its elements. The parser does not decide pair versus transform sequence versus `caps` string pair; M2 types it from its elements (spec §5.8).
- **`{ … }` is a body after a block header and a map literal in expression position.** The parser knows which from position.
- **Operator precedence per spec §5.8**: `^` binds tighter than unary minus, and its right operand may be unary.
- **Error recovery** at `;`, `}`, and declaration-keyword boundaries, so partially typed source still yields a tree with `ERROR` nodes and one bad glyph does not swallow the rest of the file.
- **The parser tracks an expected-token set**, so the message is ``expected `,` or `)`, found `deg` `` rather than `unexpected token`. A `TokenSet` bitflag accumulates in the `expect`/`at` helpers and drains into the diagnostic on failure.
- **Stable node IDs.**
  - Named declarations are keyed by `(kind, name)` under their parent.
  - Anonymous declarations are keyed by ordinal among anonymous siblings of the same kind, so inserting an anonymous sibling renumbers the later ones.
  - Plan 5 replaces the anonymous-ordinal scheme with tree-diff matching.
- **`mg fmt`** is tested for idempotence (`fmt(fmt(x)) == fmt(x)`) on the corpus. It does not have to leave the hand-aligned sample unchanged.

### M2 — HIR, validation, name resolution, type checking (4–5 days)
Every error in spec §13's "field validation", "name resolution", "type", and "path structure" classes fires with the spec's wording.

- **HIR root:** the `font` record plus `IndexMap`s of params, metrics, top-level lets, glyphs (keyed by `(name, glyphset)`), instances, and groups, and a `Vec` of kerns.
- **The four namespaces of spec §5.11**:
  - value scopes: top level, and per glyph
  - glyph and group names (font-wide)
  - glyph sets
  - segment names (per path)

  Call position resolves only against spec §5.9 functions. Near-miss suggestions come from the `mg-diag` edit-distance helper.
- **One table-driven field schema per block kind.** Each entry records the field's name, type, and required/optional status, plus:
  - default value
  - mutual exclusions (`arc`: `center` excludes `rx`/`ry`, which require each other; `large` only with `rx`/`ry`)
  - legal enum set (including `arc`'s `sweep`, and each element of a `caps` tuple)
  - value constraints checked at evaluation (`arc`'s `rx`/`ry` > 0)
  - position rules (`caps` needs `stroke` and an open path; an omitted `c1`/`c` only after a segment of the same kind; …)
  - whether a constant expression is required

  That single table produces every field-validation error, including ``unknown join "mitre"; expected one of: miter, round, bevel``. Do not scatter these checks.
- **Static type checking** over the HIR:
  - tuples typed from their elements (spec §5.8)
  - member access typed from its receiver, so `.bbox` works on a path, `glyph`, or `glyphs.<name>`
  - `int` fields checked for integrality once values are constant, and otherwise at evaluation
- Reserved-word, duplicate, and shadowing checks per spec §5.4 and §5.11. Params named after instance fields are rejected.
- **Path structural checks** (spec §6.3).
- **`codepoint`** (spec §5.6) is an `int` or `int*` field: constant expression, each value in `0`–`0x10FFFF`. It is an ordinary schema entry, not a special literal type.
- **Glyph sets** (spec §5.6): every alternate has a default glyph and no `codepoint`; every instance `glyphset:` names an existing set.
- **Kerning** (spec §12.2): `left`/`right` resolve in the glyph namespace; group-overlap and duplicate-pair checks.
- **The render predicate** is `stroke.is_some() || fill`. It lives as one method on the HIR path type.
- Required metrics are present. Glyph names are at most 63 bytes.
- When the source declares no instance, lowering inserts `instance Regular ()`.

### M3 — Evaluation engine (5–7 days)
`mg dump-graph` prints the graph, and permuting statements within a scope provably changes nothing.

- **`Value`** is an enum over the spec §5.5 types, built on kurbo (`Point`, `Vec2`, `Affine`, `BezPath`) plus our own `Line`, `Zone`, and `Rect`.
- **Graph nodes are bindings**, not every subexpression: `TopLevel(name)`, `GlyphLocal(glyph, name)`, `GlyphAdvance(glyph)`, `GlyphShift(glyph)`, `GlyphBbox(glyph)`, `PathRealized(glyph, path)`, `PathBbox(glyph, path)`, `Anchor(glyph, name)`, `Kern(index)`. Anonymous subexpressions evaluate inline inside their node.
- **`glyph.bbox` depends on every rendering path and component of the glyph**, in authored coordinates. That dependency makes `rsb: sidebear` and `lsb: (cell - glyph.bbox.width) / 2` work, and turns a path reading `glyph.advance` into a detected cycle rather than a hang.
- **`GlyphAdvance` and `GlyphShift`** follow the spec §12.1 table, from whichever one or two of `advance` / `lsb` / `rsb` the glyph declares. A declared `advance` depends only on its own expression; a derived one, and any non-zero shift, depend on `GlyphBbox`. Keeping them separate is what lets `advance: s, lsb: (glyph.advance - glyph.bbox.width) / 2` evaluate without a cycle.
- **`PathBbox` and `GlyphBbox` call into `mg-geom`**: skeleton bounds for construction paths, stroked or filled bounds for rendering paths. Until M4 lands, `mg-geom::stroke` returns an error. M3's tests therefore cover graphs without rendering-path `.bbox`, and the full sample first evaluates at the end of M4.
- Cross-glyph edges only to `glyphs.X.{advance,bbox,<anchor>}`, resolved per instance glyph set. `bbox` and anchors read this way are placed (shift added), so they also depend on X's `GlyphShift`.
- **Kahn's algorithm with a min-heap keyed on declaration index** (spec §14). When nodes remain unsorted, recover an actual cycle by DFS with a parent stack. Report every hop with file and line, plus the break hint (spec §4.3).
- **Failure containment (spec §4.6):** a failed node marks its downstream nodes failed; everything else still evaluates. A failed top-level node reports once. A build with any error writes nothing.
- Full spec §5.9 construction library, mostly thin wrappers over kurbo. Angles are radians; suffixes convert at lowering. Path queries use the `[0, n]` parameter domain of spec §5.9.
- Every domain error of spec §13 is named.
- Memoization and a dirty-set API on the graph now, exercised by a test (spec §4.5).

**Two items to schedule late in M3, neither used by the Appendix A sample:**
- The `em` suffix needs `font.em` bound first.
- `intersect(path, path)` needs a real curve–curve solve. Use Bézier clipping, in `mg-geom`; kurbo has no curve–curve intersection.

**Do not defer the clipping routine.** M4's filled-contour self-intersection check needs the same primitive, so it is on the critical path for `fill`.

### M4 — Geometry kernel (6–7 days)
`mg svg --glyph A` writes a viewable outline. Do this before touching any font table: it is how the geometry gets eyeballed.

- **Segments → `kurbo::BezPath`** per spec §6.3. Test each as a separate case:
  - `quad` becomes its exact degree-elevated cubic
  - `cube` passes through unchanged
  - an omitted `c1`/`c` reflects the previous control and makes a tangent-continuous joint
  - `arc` centre mode: the radius solve, the circular fallback on a singular system, and the non-circular-singular and no-ellipse errors
  - `arc` radii mode: the centre solve for both `large` values in both sweeps, the diameter chord within `ARC_TOLERANCE`, and the chord-too-long and non-positive-radius errors
  - `arc` realization: `⌈Δ/90°⌉` pieces with `4/3·tan(φ/4)` handles, in both sweeps. Written by hand from the spec formula, not via `kurbo::Arc`, whose piece count follows a tolerance rather than the spec's rule.
- **`close`** appends a straight line per spec §5.7, omitting it when the final endpoint is already the start point. The path is closed either way.
- **Curvature check before stroking (spec §7.2):**
  - Compare `stroke/2` against the curvature radius over each segment's interior, refining curvature extrema by root-finding.
  - Corners are excluded; interior cusps fail.
  - The error names glyph, path, segment, parameter interval, and instance.
- **Degenerate cases (spec §7.3) error before kurbo is called:** zero total arc length, a zero-length segment, `stroke <= 0`. An open path whose ends coincide is valid.
- **Stroke via `kurbo::stroke`** with per-end `Cap` mapped from the validated strings, `Join::Bevel` for every corner, and tolerance `OFFSET_TOLERANCE` (spec §14).
- **Join splicing.** kurbo's `Join::Bevel` emits, on the outer side of each corner, exactly one `LineTo` between the two offset endpoints `vertex ± r·n̂`. Both endpoints are known from the skeleton, so locate that chord in the output (endpoint match within `OFFSET_TOLERANCE`) and replace it in place:
  - `"round"`: the circular arc of radius `r` centred on the vertex, from one endpoint to the other on the outer side, as cubics at `4/3·tan(φ/4)` per ≤90° piece.
  - `"miter"`, within `MITER_LIMIT`: two lines, endpoint → miter apex → endpoint.
  - `"bevel"`, or a miter past the limit: the chord stays.

  No extra contours are emitted: an open stroke is exactly one contour, a closed stroke exactly two. Failing to find a corner's chord is an internal error, not a silent skip. Pin the kurbo version, since the splice relies on its bevel emission; a unit test asserts the one-chord-per-corner shape so an upgrade that changes it fails loudly.
- **Inner-corner trim (spec §7.4).** At each corner, including a closed path's wraparound corner, intersect the inner offset of the incoming segment with the inner offset of the outgoing segment: line–line directly, curves via the M3 Bézier clipping. Take the crossing nearest the corner, cut both pieces there, and drop kurbo's inner-join elements between them, so the inner side meets at that single point. The search is limited to the two adjacent segments' offset pieces.
  - No crossing within them, for a sharp turn beside a segment too short for its stroke: the spec §7.4 error, naming glyph, path, the segment the corner ends, and instance.
  - Tangents exactly opposite (a 180° reversal) also has no crossing, but that's a legitimate shape, not the error above — checked upfront and left untrimmed, not searched for.
  - As with join splicing, pin the kurbo version and unit-test the inner-join shape the trim expects.
- **Filled paths (spec §6.5) skip everything above.** A `fill` emits the realized closed skeleton as one contour.
- **Self-intersection detection for filled contours is a hard error (spec §8.3).** Run a pairwise curve–curve test over the contour's own segments (O(n²) on small n), reusing the M3 Bézier clipping and excluding the shared endpoints of adjacent segments. A missed crossing silently drops a lobe, so the false-negative rate is what the tests measure.
- **Contour roles (spec §8.1).**
  - kurbo returns two subpaths for a stroked closed path; the one enclosing the other is outer. Decide by point-in-contour, not by kurbo's output order.
  - Open strokes are outer.
  - Among filled contours only, a contour enclosed by an odd number of other filled contours is a counter.
- **Winding (spec §8.2):** signed area per contour, reversed where the direction disagrees with the role; `glyf` outer is clockwise in y-up. Stroke outlines can still self-overlap where the skeleton crosses itself or passes near itself (spec §7.4), so the signed area of the whole contour decides.

### M5 — Outline preparation (3 days)

Order per spec §3:

1. **Slant.** Apply the instance shear to every simple glyph outline. Conjugate component transforms to `S·M·S⁻¹` (spec §10.1). Slant precedes extrema insertion because the extrema of a sheared curve are not the sheared extrema.
2. **Insert on-curve points** at horizontal and vertical extrema (kurbo `extrema()` per segment).
3. **cu2qu on unrounded cubics** via `CubicBez::approx_spline`, tolerance `CU2QU_TOLERANCE`.
4. **Zone snap (spec §10.4):** an on-curve point with a horizontal tangent, within `ZONE_SNAP_TOLERANCE` of a metric's `.y` or `.ink`, takes that value rounded.
5. **Quantize** every coordinate and component offset, rounding half away from zero.
6. Drop segments that became zero-length, and re-run the fill self-intersection test.

The named tolerance constants live in one module, not as magic numbers at use sites.

### M6 — TTF assembly (5–7 days)
`mg build` emits TTFs that pass `ots-sanitize` and install.

- **Per instance:** bind params from defaults plus overrides, evaluate the whole font, and build one file. Each instance is an independent build (spec §12.3). The file name is `font.name` without spaces, `-`, `styleName` without spaces, `.ttf`.
- **Glyphs** via `write_fonts::tables::glyf::SimpleGlyph` from an all-quadratic `BezPath`. Confirm that write-fonts omits implied on-curve points; if it does not, omit exact midpoints before handing the path over. Set `OVERLAP_SIMPLE` on every simple glyph and `OVERLAP_COMPOUND` on every composite.
- **Components (spec §10.1):** a `glyf` composite when the conjugated 2×2 fits F2Dot14, decomposed otherwise. Acyclic check; depth limit 5.
- **`cmap`:** format 4 for the BMP, plus format 12 when any codepoint exceeds U+FFFF, with (3,1)/(0,3) and (3,10)/(0,4). Check what the write-fonts `Cmap` constructor emits, and build the subtables by hand if it differs.
- **Metrics** per the spec §12.1 table. The shift is applied to the outline before slant; composites get `translate(shift, 0)` prepended to their transform (spec §10.1). `hmtx` left sidebearings come from the final outline.
- `.notdef` at glyph ID 0, then declaration order; `post` 2.0 carrying names with `italicAngle`; `hhea` caret slope from slant. `maxp`/int16 limit checks are export errors (spec §10.5).
- **`gasp`** per spec §11.4. No `cvt`, `fpgm`, or `prep`.
- **`name`, `OS/2` weight/width class and `fsSelection`, `head.macStyle`** per the spec §12.3 naming table.

### M7 — Kerning (2 days)

Build GPOS with the write-fonts pair-positioning builders:
- one `kern` feature under `DFLT`
- glyph–glyph pairs as format 1
- group pairs as format 2 with classes kept as classes
- the format-1 subtable first, so specific pairs take precedence (spec §12.2)

Kern values come from the evaluator per instance, rounded half away from zero.

### M8 — Verification harness (3–4 days)

Per spec §15, ordered by value:

| Target | Test |
|---|---|
| Evaluator | proptest: permuting statements within a scope yields identical values. Cycle tests for self-reference, two-node, and long cycles, asserting the reported path *is* the cycle. Golden tests for `meet`/`mediate`/`project`/`polar`/`mirror` against hand-computed geometry. |
| Segments | Arc pieces against a densely sampled exact ellipse, within the 90°-piece bound; quad elevation exact; reflection and `close` per spec §15.2. |
| Stroking | Hausdorff distance against a densely sampled exact offset, at most `OFFSET_TOLERANCE`. Each spliced join against the spec §6.4 definition; every stroke yields exactly one contour (open) or two (closed). Inner corners: the outline has no self-crossing near any corner, for lines and curves at a range of angles; a segment too short for its stroke at a sharp corner produces the spec §7.4 error; a 180° reversal does not. Each degenerate case of spec §7.3 produces its named error. |
| Stroker differential | The same paths, widths, caps, and joins through `tiny-skia`'s stroker. Rasterize both and compare coverage (spec §15.4). |
| Curvature check | Fuzz random paths against random widths; assert the check fires exactly when the exact offset folds back within a segment interior (spec §15.5). |
| Fills and roles | A filled closed path emits its skeleton as one contour; a fill nested in a fill renders a hole; `stroke` + `fill` on one closed path renders solid, asserted by rasterizing and comparing coverage. Fuzz self-intersecting outlines and assert spec §8.3 fires on each. |
| Export | `ots-sanitize`, `fonttools ttx` round-trip, FontBakery `opentype` profile in CI. Render a pangram at 8–48 ppem with FreeType and diff golden rasters. `skrifa` for in-process outline assertions. |
| Determinism | Build twice, assert byte-identical output. |
| Diagnostics | The `tests/diagnostics/` corpus from M0, grown with every milestone: one broken source per error code. Assert every code has at least one case. |

**Acceptance test:** build the Appendix A sample in its three instances (Regular, Bold, Condensed), install, and set text.

---

## Sequencing

```
M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7
                      └ full-sample evaluation lands with M4's stroking
M8 grows from M0 (diagnostics corpus) and M3 (evaluator tests) onward.
```

M1–M3 gate everything. M4 carries most of the correctness risk. With kurbo doing the offsetting, what remains in M4 is:
- the curvature check
- join splicing
- the inner-corner trim
- contour roles from kurbo's output
- the fill self-intersection test

## Deferred

Editor and inverse drag (spec plan 5) · OTF/CFF and its hinting (spec §11.1–§11.3) · WOFF2 · the full spec §15 acceptance character set.

## Out of scope

Excluded by the spec itself, not deferred: variable fonts · region operators · variable width · OpenType substitutions (GSUB) · TrueType glyph instructions · any reuse or abstraction mechanism.

## Verification of this plan's output

Run from Git Bash:

```
cargo test --workspace
mg check samples/metaglyph-sans.mg
mg svg samples/metaglyph-sans.mg --glyph A --instance Bold   # eyeball geometry
mg build samples/metaglyph-sans.mg -o out/
ots-sanitize out/MetaglyphSans-Regular.ttf
python -c "import os; from fontTools.ttLib import TTFont; TTFont('out/MetaglyphSans-Regular.ttf').saveXML(os.devnull)"
mg build samples/metaglyph-sans.mg -o out2/ && diff -r out/ out2/   # determinism
```

Then install `out/MetaglyphSans-Regular.ttf` and set text in it.
