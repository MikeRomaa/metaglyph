# Metaglyph — Research: foundations and trade-offs

## Context

Conventional font editors treat a glyph as a bag of hand-placed Bézier nodes. Design intent — "the bowl of `o` is one stem-width thick," "the crossbar sits at 0.52 × x-height," "all round glyphs overshoot the baseline by 10 units" — lives only in the designer's head and has to be re-executed by hand in every glyph and every weight.

This project inverts that. A typeface is a **program**: named parameters, construction geometry, and algebraic relationships between them. Changing `stem` from 100 to 180 rebuilds every glyph. The text source is the design; the canvas is a view of it.

This document is **research**: it records the foundations of the design, the reasoning behind each decision, the alternatives considered, and what each decision gives up. The normative definition of the language and pipeline is `2-specification.md`; where the two disagree, the specification governs. Section numbers below refer to this document unless marked "spec".

---

## 1. Locked decisions

| Axis | Decision | Consequence |
|---|---|---|
| Geometry resolution | **Pure directed construction.** One definition per value; a dependency graph; topological evaluation. No equations, no free variables, no linear elimination, no numeric solver. | Deterministic; source fully determines output; no branch flipping; every operand known when used, so nonlinear operations need no special rules. No solver means no Gaussian elimination, no pivot selection, no inconsistency provenance, and no "inconsistent equation" error class — a value cannot have two definitions. Cost: circular definitions are illegal (§5.1). |
| Language shape | **Uniform declarative blocks.** Two forms only: `let name = expr;` and `kind name? (config)? { declarations }?`. No infix structure — no path operators, no clause keywords, no trailing modifiers. Infix survives only for arithmetic inside expressions. | Every construct is a keyed property bag, so a structured edit is "set this key" and generated text is indistinguishable from hand-written text — which is what the DSL-canonical decision requires (§6.1, §10.3). Cost: paths are more verbose than METAFONT's `..` notation. |
| Shape model | **SVG semantics.** A path may be stroked — a scalar `stroke`, equivalently a circular pen of that diameter — filled, or both. No custom pens, no pen angle, no razors. | Drops the entire support-function/convexity apparatus from §8. Caps and joins are the standard SVG sets. In exchange, contrast must be written explicitly, between paths or as a `fill`, rather than inherited from a pen shape (§7.3). |
| Output | **Static TTF / OTF / WOFF2**, plus kerning and hinting. No OpenType substitutions, no variable fonts. | No interpolation-compatibility constraint on the geometry model — a significant simplification. Multiple weights ship as separate instances of one source. |
| Text ↔ editor | **DSL is canonical.** The editor is a projection; every edit is a structured transform on the syntax tree, followed by re-evaluation. | Requires a lossless (comment- and format-preserving) syntax tree, stable node identity, and an "inverse drag" mechanism (§10.2). |
| Stroke width | **One constant `num` per path, the `stroke` field.** No variable width, no profiles, no per-segment overrides. | Deletes the variable-width envelope entirely (§8) — the subtlest correctness risk in the geometry kernel. Cost: **contrast between strokes exists only between paths, never within one** (§7.3), so monolinear and low-contrast designs are served by stroking alone; a high-contrast shape must be authored as a `fill`. |
| Fill | **`fill` is an independent path property, freely combined with `stroke`** — SVG semantics. A filled path's own outline is the contour; it bypasses offset generation. | Lifts the contrast ceiling without reinstating variable width. Costs: a filled outline is parametric only insofar as its coordinates are expressions (§7.3); it yields no stem hints (§12.3); and it can self-intersect, which reintroduces a curve–curve test as a detector — though not as a resolver (§9.2). |
| Caps and joins | **SVG sets, as validated string literals, no arguments.** Caps: `"butt"` (default), `"round"`, `"square"`. Joins: `"miter"` (default), `"round"`, `"bevel"`. Miter limit is a fixed implementation constant. | Universally understood, heavily precedented, and no `natural` case to explain — with a circular pen `natural` *is* `round`. |
| Script scope | **Arbitrary Unicode codepoints + component reuse.** No complex-script machinery: no contextual reordering, no cursive attachment, no mandatory mark-attachment anchors. | `cmap` must cover the full Unicode range (§11.5). Composites carry accent and derived-glyph reuse. Anchor-based mark positioning is an optional later addition, not a design constraint. |
| Overlaps | **Always kept.** Overlapping contours of consistent winding are what ships. There is no overlap-removal flag, because removing overlaps *is* a union and there are no region booleans. | Removes the pipeline's most numerically fragile code entirely (§9.1). Cost: TrueType hinting behaves marginally better without overlaps, and some old tools mishandle them. |
| Region operators | **None.** No `trim`, `union`, `difference`, `intersection`, or half-planes — and no filled-area value in the language at all. Stroking produces ink inside the compiler; the source reaches extent only through `.bbox`. | Collapses §1.1 to one cost tier and deletes the planar-arrangement stage. Shaping comes from caps, final-segment direction, and overlapping paths (§1.1). Cost: a cut cannot be decoupled from the stroke tangent. |
| Overshoot | Declared once per metric zone; applied explicitly per glyph by reading `.ink` instead of `.y`. | Zones stay the single source of truth for both geometry and hinting (§12.1), while the designer controls which glyphs overshoot. |
| Italics | `slant` is an ordinary transform in the language, applicable per instance. A *true* italic is a separate glyph set selected by instance, not a sheared roman. | No special italic machinery. Instances may override which glyph definitions they build (§13.3). |

### 1.1 Consequence that needs flagging up front

A round-ended stroke cannot, by itself, produce a flat-sided slab serif or an arbitrary terminal. There is **no cutting or subtraction in the language at all** — no `trim`, no `union`, no `difference`. Four mechanisms cover the shaping need; the first three are free:

| Mechanism | How | Handles |
|---|---|---|
| **Caps and joins** (§7.3, §8.3) — computed during the offset walk | Zero intersections for all three caps and two of three joins; `miter` needs one local curve–curve solve | Butt / round / square ends, asymmetric ends, corner treatments |
| **Segment direction sets the cut angle** | A `"butt"` cap is perpendicular to the tangent, and the final segment's last control point sets that tangent — so any perpendicular-to-stroke cut is available at whatever angle you choose | Angled and sheared terminals, flat cuts on diagonals |
| **Overlapping paths** | Overlaps are kept and nonzero winding is order-independent (§9.1), so adjacent paths fuse for free | Slab serifs, flat apexes and flat tops across several strokes, abrupt width changes, compound terminals |
| **`fill` on a closed path** (§7.2) | The path's own outline is the contour; no offsetting, so no cap, join, or curvature limit applies to it | Anything the three above cannot reach: off-perpendicular cuts, contrast within one shape, arbitrary outlines |

The second row is easy to underestimate. Because the end tangent is yours to choose, a `butt` cap is not restricted to any particular angle — it is restricted only to being *perpendicular to the stroke there*.

**What stroking cannot express:** a cut whose angle is **decoupled** from the stroke's tangent at that point — a curved arm cut dead vertical while the arm is still turning. Two answers. End the skeleton with a short straight segment oriented so its perpendicular is the cut you want, accepting a slight corner where it meets the curve; real terminals often have such a break. Or author the shape as a `fill`, where the outline is placed directly and no tangent constrains anything.

**The fourth row is not free, and the price is not code.** A stroked skeleton with `stroke: stem` rebuilds when `stem` changes. A filled outline responds to nothing unless every coordinate is itself an expression over the parameters — so `fill` trades the parametric guarantee for expressive reach, one path at a time. It also yields no stem hints (§12.3) and can self-intersect, which §9.2 must detect. Reach for it where stroking genuinely cannot express the shape, not to avoid writing a skeleton.

**Slab serifs** are a second path — a short horizontal path with its own width, overlapping the stem. A cap can only *close* a boundary; it cannot add width, and a slab is wider than its stem.

---

## 2. Pipeline

```
source text
  → lex / parse                    → lossless syntax tree (CST)
  → lower                          → AST + scope tree
  → build dependency graph         → per-glyph node sets
  → topological evaluation         → all values known (§5)
  → geometry realization           → skeleton paths (cubic Béziers) + stroke widths
  → offset generation              → per-stroke outline contours (§8)
  → curvature check + winding      → tight-curvature errors; contour direction (§9)
  → extrema insertion + quantize   → integer design-unit outlines (§11.2)
  → curve conversion               → quadratic (glyf) or cubic (CFF2) (§11.3)
  → hint derivation                → blue zones, stem snaps, instructions (§12)
  → metrics / kern build          → hmtx, GPOS (§13)
  → table assembly                 → TTF / OTF
  → repackage                      → WOFF2
```

Each arrow is a pure function. That matters for the editor: re-evaluation after an edit re-runs a suffix of the pipeline, and caching keys off the dependency graph.

The spec's pipeline (spec §3) refines this sketch in two places. Instance slant is applied before extrema insertion, because the extrema of a sheared curve are not the sheared extrema. Quantization runs *after* curve conversion, because cu2qu places off-curve points at arbitrary positions and rounding first would not yield integer output.

---

## 3. Units and coordinate space

- One internal unit = one **design unit**. All reals are `f64` in design space.
- `em` is declared once, in the `font` directive (`font (name: …, em: 1000)`). Recommend 1000 for CFF-flavored output, 2048 for TTF if legacy hinting matters; the spec must not hardcode either.
- Literals may be suffixed: `100` (design units), `0.25em` (fraction of em), `20deg` / `0.35rad` (angles), `50%` (fraction — only valid where a ratio is expected).
- Integers have three extra spellings, none suffixable: hex `0x41`, codepoint `U+0041`, and character `'A'` (any Unicode scalar value, not only ASCII). All four of `65`, `0x41`, `U+0041`, `'A'` are the same plain number. A codepoint is not a distinct type: a separate `codepoint` type bought nothing but a conversion rule, and `'é'` reads better than `U+00E9` in a `codepoint:` field. Each spelling is still checked where it is written — `U+110000`, `''`, and `'ab'` are syntax errors — so a typo in a literal is caught at the literal, not at `cmap` assembly.
- Y is up. Baseline is y = 0 by convention but is a declared metric, not a built-in constant.
- Angles are counter-clockwise from +x.

---

## 4. Type system

| Type | Notes |
|---|---|
| `num` | f64 scalar. |
| `pair` | 2-vector. Used for points, directions, offsets. |
| `point` | Alias for `pair` used in position contexts. No distinct semantics; keeps code readable. |
| `transform` | Affine 2×3. Composable. |
| `path` | Ordered chain of SVG-style segments (`start`, `line`, `quad`, `cube`, `arc`), open or closed. Each segment names the point it arrives at and carries only the fields its own kind admits (§7.2.1). Realized as cubic Béziers. |
| — | There is no width or profile *type*. The stroke width is a single `num` on the path, the `stroke` field (§7.3); it does not vary along a path. |
| `cap` | End treatment for an open stroke. A **closed set of string literals**, no arguments, not user-extensible: `"butt"` (default), `"round"`, `"square"` (§7.3). Settable per end: one string for both, or a `(start, end)` 2-tuple. |
| `join` | Corner treatment where two segments meet. Likewise a string literal: `"miter"` (default), `"round"`, `"bevel"`. Settable per path and per segment. |
| — | There is no filled-area type. Stroking produces ink inside the compiler, but the language never names a region value: it cannot be combined, cut, subtracted, or measured for area. Extent is reachable only through `.bbox` on a `path` or a glyph (§6.5). |
| `line` | Infinite line (construction only). Represented as point + direction. |
| `bool`, `string`, `int` | Ordinary. `int` only for counts and codepoints. |
| `list<T>` | Homogeneous sequence. Needed for glyph groups and enumerated config values. |

Two types were considered and left out. **`circle` / `arc`** (centre + radius, construction only) would serve `tangentFrom` and `fillet`; they return with those constructors if a design needs them (§6.3). **`record`** (a named field aggregate) exists only to let a function return several values, and the language has no user-defined functions (§6.4).

Type checking is static. **No value is ever partially determined** — every name is bound once, by an expression over already-bound names (§5.1), so by the time any operation runs its operands are concrete. There is no "unknown" state for any type, which is why nonlinear operations need no legality rules (§5.4).

---

## 5. Evaluation semantics — directed construction

The engine is a **topological sort over a dependency graph, then evaluation**. That is the whole of it. This section is short by design, and that shortness is the single biggest structural win in the spec.

### 5.1 One definition per value

Every name is bound exactly once, by an expression over names already bound. `=` is *definitional*, never an equation:

```
let hairline = stem * contrast;
let barY     = capHeight.y * 0.52;
let apex     = (w/2, capHeight.ink);
```

(`stem` and `contrast` are top-level `param`s, `capHeight` a top-level `metric` — all referenced bare, §6.5.)

There is no assignment operator, no equation solving, no free variables, and no way to state a partial fact about a value. A name's meaning is fully readable from its own definition — you never hunt elsewhere in the file to learn what determines something.

The system is a **parametric construction** language — closer to a spreadsheet or OpenSCAD than to a CAD sketch solver.

**Why no equations.** A language like METAFONT needs free variables (`whatever`) because it lacks constructors for partially-determined geometry. Audited against real Computer Modern source, `whatever`'s uses reduce to two patterns — line intersection, and "point on a line, pinned by a separate constraint" — and both are intersections that a constructor expresses directly:

| Equation-based idiom | Directed equivalent |
|---|---|
| `z = whatever[a,b] = whatever[c,d]` | `meet(lineThrough(a,b), lineThrough(c,d))` |
| `z = whatever[a,b]` with `z.x` pinned elsewhere | `meet(lineThrough(a,b), vline(serifWidth))` |
| `p - q = whatever * dir(θ)` | `polar(p, len, θ)`, or `meet(lineAt(p,θ), hline(q.y))` |
| the same free fraction used twice | `let barPos = 0.35;` — it's a parameter, not an unknown |

Each right-hand column names its intent, is one statement rather than a chained equation, and resolves in closed form rather than through a solver. Given constructors, free variables buy nothing but the ability to spread one value's determination across several statements — which is precisely what makes source hard to read and hard to diagnose.

**The cost of this choice:** mutual and circular definitions are illegal. Where a designer thinks circularly — "these two points are symmetric about an axis midway between them" — they must pick a direction: axis from points, or points from axis. That is usually easy and often clearer, but it is a real restriction, and it is why §5.3's cycle diagnostic has to be excellent.

### 5.2 The algorithm

1. **Resolve names.** Every expression's free identifiers become edges into the dependency graph. Unresolvable name → error naming it and the enclosing scope.
2. **Topologically sort** the graph.
3. **Evaluate** in that order. Every operand is known by construction when its operation runs.

Order-independence in the source is free: the graph is built from references, not statement order, so a definition may appear before or after what it depends on. This is what allows a block's config to reference its body and vice versa (§6.1).

No iteration, no convergence, no tolerance, no pivoting. Evaluation is `O(V + E)`.

### 5.3 Cycles are the only structural error

With one definition per value, the two remaining failure modes are an **unresolved name** and a **cycle**. Both are structural and both are reportable precisely.

Cycle diagnostics are the highest-value error in the system. Report the **full cycle as a path**, not just "cycle detected":

```
error: circular definition
  stemAxis  → depends on → leftEdge   (glyph n, line 14)
  leftEdge  → depends on → stemAxis   (glyph n, line 11)
  hint: break the cycle by deriving one from a parameter —
        `let stemAxis = ...` independent of leftEdge
```

Because circular definitions are illegal in exactly the place a designer may naturally think circularly (§5.1), this message is doing real work and must be built in from the first commit, not retrofitted.

### 5.4 Nonlinear operations need no special treatment

Every operand is known when its operation runs, so `sqrt`, `atan2`, rotation by an arbitrary angle, curve–curve intersection, and arc-length parameterization are all ordinary function calls with no preconditions beyond their own domains.

This is worth stating because an equation-based engine cannot make that guarantee — it must restrict which nonlinear combinations are legal (multiplying two unknowns, dividing by an unknown) and carry an error class for violations. Directed evaluation makes the question disappear rather than answering it.

### 5.5 Scoping

There are exactly **two scopes**: the file's top level, and each `glyph` body. A glyph sees top-level declarations and its own `let`s, all bare, with shadowing forbidden so every bare name has one binding (§6.5). Nothing else exists — no user-defined functions, no nested scopes, and no way for one glyph to see another's locals.

Per-glyph scoping is non-negotiable for three reasons: errors stay local to one glyph; a broken glyph doesn't block the font; and per-glyph graphs are small (tens to low hundreds of nodes), keeping evaluation trivially fast and enabling per-glyph caching in the editor.

The flatness is worth noting because it is what keeps the whole engine as small as §5.2 describes. Adding a reuse construct later (Appendix A.2) is the one change that would introduce a third scope, and it is the main reason that decision is deferred rather than guessed at.

### 5.6 Incremental re-evaluation

An edit dirties the node it touches and everything downstream of it in the graph. Re-evaluate that subgraph only. With per-glyph scoping (§5.5) this bounds a typical edit to one glyph's graph — which is what makes the editor feel live (§10.3).

---

## 6. The language

### 6.1 Two forms, and only two

The whole grammar is:

```
let <name> = <expression>;
<kind> <name>? ( <config> )? { <declarations> }?
```

**Parens are configuration — what the thing *is*. Braces are declarations — what it *contains*.** Both parts are optional, so a construct with no declarations has no braces at all, and one with no configuration has no parens.

| Rule | |
|---|---|
| `let` statements | terminated by `;` |
| Config fields | separated by `,`, trailing comma allowed |
| Declarations in a body | no separator; blocks are self-delimiting |
| Body ordering | significant only where the construct is a sequence (path segments) |

```
param stem     (default: 100, range: 20..300)           // config only
param sidebear (default: 44,  range: 0..140)            // config only
metric capHeight (y: 700, overshoot: 12)                // config only
instance Bold  (stem: 180, contrast: 0.30)              // config only

glyph A (codepoint: U+0041,
         advance: glyph.bbox.width + 2*sidebear) {      // both
  let w    = stem * 6;
  let apex = (w/2, capHeight.ink);
  path leg (stroke: stem) {
    start (at: apex)
    line  (to: (0, 0))
  }
}
```

Every bare name above is declared somewhere in the snippet — `stem`, `sidebear`, `capHeight` at top level, `w` and `apex` in the glyph. The only prefixed name is `glyph.bbox`, which is built in (§6.5).

Every construct follows the same shape at every depth — `glyph`, `path`, `start`, `line`, `quad`, `cube`, `arc`, `param`, `metric`, `group`, `kern`, `instance`. There is no second mechanism to learn and no per-construct exception.

#### Top-level directives

There is no wrapper block. `font`, `param`, `metric`, `let`, `glyph`, `instance`, `group`, and `kern` are all **top-level directives**, and everything declared at top level is visible everywhere **by its bare name**:

```
font (name: "Metaglyph Sans", em: 1000)

param stem     (default: 100,  range: 20..260)
param contrast (default: 0.86, range: 0.40..1.00)
param sidebear (default: 44,   range: 0..140)

let   hair    = stem * contrast;
let   originX = sidebear + stem/2;

metric capHeight (y: 700, overshoot: 12)

glyph A (codepoint: U+0041) {
  let w    = stem * 6;
  let apex = (originX + w/2, capHeight.ink);   // every name here is declared
  path leg (stroke: stem) { … }
}
```

**Shadowing is an error.** A glyph may not declare a name that already exists at top level. That is what keeps bare resolution unambiguous — every bare name has exactly one binding, found without knowing which scope you are in. It is enforced by the same check that rejects reserved words as names (§6.5).

The cost is real and worth stating: a glyph can no longer name a path after a parameter. `path stem` beside `param stem` is now a collision, so the sample in Appendix A calls its vertical `upright`. That is the price of dropping the prefix, and the prefix's noise — `stem` many times per glyph — was the larger cost.

**Multi-file composition** falls out: a face spans several files, only one of which carries the `font` directive, and the rest contribute params, lets, and glyphs at their own top level. Files compose by concatenation with no include mechanism.

**A block's body is a grouping, not a scope.** Names declared inside live in the block's own namespace, which is what lets a header expression reference a body declaration. The forward reference is fine because the dependency graph is built from references rather than statement order (§5.2) — config may depend on a body declaration and vice versa, as long as there is no cycle. An implementer who assumes the body opens a scope will break every `advance:` expression that mentions a local, so this is stated rather than left to inference.

**One parsing note for §17.1:** `{ … }` is a body after a block header and a **map literal** in expression position (`joinAt: { elbow: "bevel", wrist: "round" }`). The two never occur in the same position, so the grammar is unambiguous, but the parser must be written knowing it.

#### Structure is never infix

No path operators, no clause keywords (`with`, `caps`, `joins`, `at`), no trailing modifiers (`overshoot 10`). This is what makes the editor's job tractable: every construct is a keyed property bag, so a structured edit is "set this key," and generated text is indistinguishable from hand-written text.

**Expressions keep infix arithmetic**, because `mul(add(stem, 3), 4.2)` is not an improvement on `(stem + 3) * 4.2`. The line is: infix for *arithmetic on numbers, pairs, and transforms*; blocks and function calls for everything structural.

### 6.2 Expression operators

| Category | Operators |
|---|---|
| Arithmetic | `+ - * / ^`, unary `-` |
| Pair | `+ -` componentwise, `*` `/` by scalar, `.x` `.y` accessors, `(a, b)` constructor |
| Comparison | `< <= == != >= >` |
| Boolean | `and or not` |
| Member access | `.` for record fields, block properties, and metric sub-values (`xHeight.ink`) |

Mediation loses its `t[a,b]` bracket form and becomes the ordinary function `mediate(a, b, t)` — the bracket was infix structure, and it read as indexing to anyone who hadn't used METAFONT.

### 6.3 The construction library — minimal, extended on demand

Per the decision to start minimal: ship the set below, add constructors when a real glyph needs one. Every one returns a **fully determined** value; there are no partial-fact constructors, because there are no free variables to leave dangling (§5.1).

**Scalar:** `abs sign floor ceil round min max sqrt sin cos tan asin acos atan2 exp log clamp lerp`

**Pair / vector:** `length angle dir(θ) unit dot cross perpendicular`

**Transforms** — `identity`, `translate(dx, dy)`, `rotate(θ)`, `scale(s)` or `scale(sx, sy)`, `slant(θ)`, `reflect(line)`. Applied with `apply(t, value)`.

**Composition is a parenthesised sequence, applied in reading order:**

```
transform: (rotate(180deg), translate(w, h))      // rotate first, then translate
transform: rotate(180deg)                          // a single transform needs no sequence
```

No infix composition operator. A sequence *is* one transform — the parens read as "this whole thing," not as a collection, which is why it is not `[…]`.

Two notes for §17.1. A sequence is written exactly like a pair literal; the parser emits one tuple node and the type checker resolves it from the element types: transforms make a sequence, two numbers make a pair (spec §5.8). Resolving by the *field's* expected type was the first idea and fails for a `let`, which has no expected type. And `translate` takes two scalars rather than a pair, purely to avoid `translate((w, h))`.

**Point construction — the core set that replaces free variables:**
`meet(l1, l2)` line intersection · `mediate(a, b, t)` · `project(p, l)` perpendicular foot · `polar(p, len, θ)` · `mirror(p, l)`

**Lines:** `lineThrough(a, b) lineAt(p, θ) hline(y) vline(x)` — the constructor is `lineThrough`, not `line`, because `line` is a segment declaration keyword (spec §5.9)

**Path queries:** `pointAt(p, t) directionAt(p, t) curvatureAt(p, t) arcLength(p) pointAtLength(p, s) intersect(a, b) subpath(p, t0, t1) reverse(p) extrema(p)`

**Extent is a member, not a function:** `p.bbox`, `glyph.bbox`, `glyphs.<name>.bbox`. There are **no region operators** — no `trim`, `union`, `difference`, `intersection`, no half-plane constructors (§1.1) — and no area query, because with overlaps kept there is no union available to make one meaningful.

**Reductions:** `sum maxOf minOf` over `list<num>` — needed for optical sidebearings.

**Enum-valued fields** take validated string literals, never keywords (spec §5.5): caps `"butt"` `"round"` `"square"`; joins `"miter"` `"round"` `"bevel"`; `align` `"top"` `"bottom"`.

**Direction constants** are genuine bare values of type `pair`, not strings: `up` `down` `left` `right`, interchangeable with `dir(θ)`.

**Deliberately deferred**, to be added when a glyph demands them rather than speculatively: `circleAt` / `circleThrough`, `tangentFrom`, `bisector`, `fillet`, `leftEdgeAt` / `rightEdgeAt` (optical sidebearings), `arcLength`-based distribution helpers. The risk of starting minimal is discovering mid-build that a common construction has no expression; the mitigation is that the two patterns carrying most of the weight are line intersection and point-on-line-plus-a-constraint (§5.1), and `meet` covers both.

### 6.4 What is deliberately absent

- **No free variables, no equations, no assignment.** (§5.1)
- **No user-defined functions, macros, shapes, or glyph inheritance** — no abstraction mechanism of any kind. Every glyph is written out in full. Deliberate and deferred, not overlooked: Appendix A.2 records the options and the constraints any later choice must respect.
- No recursion and no loops. Two scopes only (§5.5), so the dependency graph is finite by construction.
- No raster/`picture` operations (METAFONT has them; an outline font does not need them).
- No user-defined operators, and no user-definable caps or joins.
- No infix path construction, and no separate `stroke` or `draw` construct — `path` is the only shape block (§7.2).
- **No opaque-text block anywhere in the grammar**, and therefore no OpenType substitutions (§13.3). Kerning has its own directives.

### 6.5 Namespaces and the global environment

One rule: **user declarations are bare; built-in namespaces are prefixed.**

#### Bare — everything a source file declares, and nothing else

Every top-level `param`, `metric`, and `let` is visible everywhere by its bare name (§6.1), and so is every `let`, `path` name, and `anchor` name inside a glyph. Segment names are not values: they live in a per-path namespace read only by `joinAt` (spec §5.11), so two paths may both name a segment `top`. Metrics expose `.y`, `.ink`, and `.overshoot`, so `capHeight.ink` reads directly (§7.1).

```
capHeight.y * 0.42 + stem/2        // a metric and a param, both declared
```

The converse is the part that matters for reading code: **a bare name is always something the source declared.** If an identifier has no `param` / `metric` / `let` / `path` declaration to point at, it is either prefixed, a function call, or a built-in constant (`up`, `identity`, …) — never an implicit global. So `capHeight` above is bare *because* a `metric capHeight` exists somewhere in the source, not because the system supplies one.

Note that five metric *names* are reserved and required — `baseline`, `xHeight`, `capHeight`, `ascender`, `descender` — because the exporter must identify those specific zones (§7.1, §13.1). They are still user-declared, with user-chosen values; only the spelling is fixed.

**Shadowing is an error**, which is what makes this unambiguous — a glyph may not declare a name that exists at top level, so every bare name has exactly one binding and you never need to know which scope you are in to resolve it. Params are top-level only; a glyph cannot declare one, because the design space belongs to the typeface.

The cost is that a glyph cannot name a path after a parameter — `path stem` beside `param stem` collides (§6.1).

#### `font.*` — the font directive's own attributes

Only the built-in attributes of the `font` directive, **not** params, metrics, or lets:

| Name | Type | |
|---|---|---|
| `font.em` | `int` | Units per em |
| `font.name` | `string` | Family name |
| `font.version` | `string` | |

#### `glyph.*` — the current glyph, implicit and read-only

| Name | Type | |
|---|---|---|
| `glyph.name` | `string` | |
| `glyph.codepoints` | `list<int>` | |
| `glyph.advance` | `num` | The resolved advance: declared, or derived from `lsb` / `rsb` (§13.1) |
| `glyph.bbox` | `rect` | Bounds of all ink from this glyph's rendering paths, in authored coordinates (before the §13.1 shift) — the one place the language needs a name for "all of it" (§13.1) |

Reading `glyph.advance` from a path, while `advance` is itself defined from that path, is a cycle — caught and reported by §5.3 like any other.

#### `glyphs.*` — other glyphs

`glyphs.<name>` reaches another glyph, for component placement and anchor arithmetic: `glyphs.acute.top`. Distinct from `glyph.*` (the current one) by the plural, which is deliberate — the two would otherwise be easy to confuse.

**Only three things are readable from outside a glyph:** `advance`, `bbox`, and its declared `anchor`s. Its `let`s and paths are not — they are that glyph's internal business, and exposing them would make every glyph's private naming part of the font's public surface. This keeps the cross-glyph dependency graph narrow, which matters for the per-glyph caching in §5.6.

#### `instance.*` — which instance is being built

| Name | Type | |
|---|---|---|
| `instance.name` | `string` | |
| `instance.slant` | `num` | Angle; `0` when unset |

For the rare per-instance conditional. Most per-instance variation should go through swept params or a `glyphset` (§13.3) rather than branching on the name.

#### `math.*` — constants

`math.pi`, `math.tau`, `math.e`. Rarely needed given angle literals (`20deg`), but cheap to provide.

#### Functions are bare; enum values are strings

**Functions** (§6.3) are bare. A bare identifier followed by `(` is a call, so functions occupy a syntactically distinct position and cannot collide with values.

**Enum-valued fields take string literals**, not keywords: `caps: "round"`, `joins: "miter"`, `align: "bottom"`. The legal set is enforced by each field's own validator (spec §5.5).

That choice removes an entire mechanism. The alternative — bare keywords resolved against the field's expected type — is workable but it forces a type-directed name lookup into the resolver purely so that `round` can be a cap value, a join value, **and** the function `round(x)` at once. With strings there is no relationship between `"round"` and `round`, so no rule is needed and the resolver stays a plain scope chain.

It also shrinks the reserved list: `butt` `round` `square` `miter` `bevel` `top` `bottom` are free to use as declaration names, which matters because `top` is a natural local (`let top = capHeight.y;`) and Appendix A uses it in nearly every glyph.

One collision is *not* solved this way and needed a rename instead: `line` is a segment declaration keyword, and a `line(a, b)` constructor would be the same token sequence — identifier followed by `(`. Hence `lineThrough` (spec §5.9). Strings do not help there because the conflict is between a declaration and a call, not between a value and a name.

#### Reserved words may not be used as declaration names

**`up` `down` `left` `right` `true` `false` `and` `or` `not`, the declaration keywords, and the namespace roots are reserved** (spec §5.4). Directions stay reserved because they are genuine `pair` *values* usable in expressions — `right` and `dir(0deg)` are interchangeable, as in `c1: p + right * k` — so they do live in the name environment, unlike the string-valued enums.

#### Summary

| Prefixed | |
|---|---|
| `font.*` | the `font` directive's attributes only |
| `glyph.*` | the current glyph's implicit values |
| `glyphs.*` | other glyphs: `advance`, `bbox`, and declared anchors |
| `instance.*` | which instance is building |
| `math.*` | constants |

Everything else — params, metrics, lets, functions, built-in constants — is bare. The prefixed set is closed and built-in, so a reader can tell at a glance that anything prefixed came from the system and anything bare was declared in the source.

---

## 7. Geometry vocabulary

### 7.1 Construction geometry (never renders)

This is the vocabulary a construction system could offer. The spec ships points, lines, metric guides, construction paths, and measurements; the rest are candidates, recorded here with what they would need.

1. **Points** — free (literal) or derived (midpoint, mediation, projection, intersection, polar offset, reflection).
2. **Lines** — through two points; point + angle; parallel at distance; perpendicular through point; tangent from point to circle; angle bisector. Infinite by default; `segment` variant for length queries.
3. **Circles / arcs** — center+radius, through-3-points, tangent constructions.
4. **Metric guides** — named horizontal alignment zones, declared at top level and referenced bare:

   ```
   metric capHeight (y: 700, overshoot: 12)              // align: "top" by default
   metric baseline  (y: 0,   overshoot: 10, align: "bottom")
   metric figHeight (y: 680, overshoot: 10)              // free-form, blue zone only
   ```

   **Config:** `y` (the flat position, required), `overshoot` (an amount, default 0), and `align: "top" | "bottom"` — which side of `y` the overshoot falls on. `top` is the default.

   **Accessors:** `.y` for the flat position and `.ink` for where a round glyph reaches — `y + overshoot` when `align: "top"`, `y − overshoot` when `align: "bottom"`. Plus `.overshoot` for the raw amount.

   A single `.ink` accessor is deliberate. A `.top`/`.bottom` pair would imply the overshoot direction is inferable from which metric it is — true for the five reserved names, false for any free-form zone, and it leaves the exporter unable to tell a `BlueValues` zone from an `OtherBlues` one. **`align` is the one piece of information that makes both the accessor and the blue-zone emission well-defined** (§12.1), so it is declared rather than guessed.

   **Five names are reserved and required:** `baseline`, `xHeight`, `capHeight`, `ascender`, `descender` — the exporter must identify these specifically for `OS/2` and `hhea` (§13.1), and a font omitting one is an error. Everything else is free-form and contributes a blue zone only. So the set is **limited in meaning but open in count**: five the compiler understands, plus as many extra zones as the design wants.

   This is also why metrics are a directive rather than `font()` properties: each carries three config values, the set is open-ended, and they are referenced bare like params. As config fields they would need a nested map, a second mechanism for the free-form ones, and a `font.` prefix that nothing else uses.
5. **Vertical guides** — sidebearing lines, stem centerlines, italic angle line.
6. **Axes** — mirror and rotational symmetry; also drive repetition.
7. **Grid / modular unit** — a declared quantum for snapping and for expressing widths as multiples.
8. **Measurements** — an ordinary `let` whose value the editor displays on canvas (`let bowlWidth = length(l - r);`). No separate construct: any named scalar or pair can be shown as a dimension, which is one more thing the directed model gets for free.

### 7.2 `path` is the only shape construct

There is no separate stroke block and no `draw:` list. A single `path` block carries the skeleton, the thickness, the end and corner treatments, and the shaping — and one rule decides whether it renders:

> **A path renders if it declares `stroke`, `fill`, or both. A path declaring neither is construction geometry.**

```
path <name> (
  follows:  <path>,                   // reuse another path's skeleton
                                      //   instead of declaring segments

  // rendering — omit both and the path never renders:
  stroke:   <num>,                    // constant along the whole path
  fill:     <bool>,                   // requires `close`; combines with stroke
  caps:     <string> | (<string>, <string>),   // both ends, or (start, end)
  joins:    <string>,
  joinAt:   { segmentName: <string>, … },
  enabled:  <bool>,                   // conditional rendering
) {
  start <name>? ( at: <point> )                                // exactly one, first
  line  <name>? ( to: <point> )                                // §7.2.1
  quad  <name>? ( c: <point>?, to: <point> )
  cube  <name>? ( c1: <point>?, c2: <point>, to: <point> )
  arc   <name>? ( to: <point>, sweep: <string>,
                  center: <point> | rx: <num>, ry: <num>, large: <bool>? )
  …                                   // order is significant
  close                                                        // at most one, last
}
```

That is the complete shape vocabulary. There is no shaping section — no `trim`, no `subtract` (§1.1).

A separate stroke construct would buy two things — a skeleton that doesn't render, and a region composable before rendering — and both are covered here as properties instead:

| Need | Mechanism |
|---|---|
| Skeleton that doesn't render | omit both `stroke` and `fill` |
| Same skeleton at two stroke widths | two paths, one with `follows:` |
| Filled interior | `fill: true`, with `close` |
| Filled shape with a stroked border | both on one path |
| Flat-cut terminal | the final segment's last control point, or an overlapping path (§1.1) |
| Render order | declaration order |
| Conditional rendering | `enabled:` |

**On render order:** it does not affect appearance, because overlaps are kept and nonzero winding is order-independent (§9.1). It fixes only contour order in the output, and declaration order is deterministic — so an explicit render list would buy nothing.

**`.bbox` disambiguation:** `p.bbox` is the bounds of *whatever the path is* — skeleton bounds for a construction path, inked bounds for a rendering path. Sidebearings want inked, and `bowl.bbox.width` gives it.

It is also the only way to reach a curve's extreme. A bowl's rightmost ink lies at the curve's x-extremum, which is not in general any declared point, and `extrema(p)` cannot supply it — that returns a list of parameters and the language has no indexing (§6.4), so there is no way to evaluate the path at the one that matters.

### 7.2.1 Segments, not knots

A path body is a chain of **segment declarations** in the shape of SVG path data — `start` (moveto), then `line`, `quad`, `cube`, and `arc` (lineto, the two curvetos, and an elliptical arc) — each naming the point it *arrives at*. One declaration per point.

```
path bowl (stroke: stem) {
  start (at: t)
  arc   (center: ctr, to: r, sweep: "cw")
  arc   (center: ctr, to: b, sweep: "cw")
  arc   (center: ctr, to: l, sweep: "cw")
  arc   (center: ctr, to: t, sweep: "cw")
  close         // already back at t: nothing appended
}

path upright (stroke: stem) {
  start (at: (0, capHeight.y))
  line  (to: (0, 0))
  line  (to: (w, 0))       // corner at (0,0) — nothing to declare
}
```

**Why segments rather than knots.** The properties a path needs do not all belong to points. Control points are handles on a segment, an arc's centre and sweep describe a segment, and the segment *type* is obviously a segment's own. Putting them on a point makes `knot (at: p, segment: line, c1: q)` grammatically legal and meaningless. With segment-typed declarations, **`line` simply has no control-point field**, so the invalid state is unrepresentable rather than merely discouraged.

This is the same shape as SVG path data — moveto, lineto, curveto, arc — which is consistent with the SVG stroke semantics of §7.3.

#### Explicit geometry, no inheritance

Every segment's curve is fixed by its own fields and the current point. `quad` and `cube` name their Bézier control points; `arc` names a sweep direction and either the centre of an axis-aligned ellipse, whose radii then follow from the two endpoints, or the radii, whose centre then follows. A joint is smooth exactly when the tangents on either side agree, and a corner happens wherever they don't — at a `line` joint that means nothing is declared either way.

**Arcs have two modes, and `center` excludes `rx`/`ry`.** An axis-aligned ellipse has four unknowns — centre and two radii — and the two endpoints give two equations. Each mode declares the other half, so neither is over-determined and nothing is stated twice (§5.1).

- **Centre mode** is the default for letterform bowls, which are authored from a centre — the `o`'s middle, a bowl's axis — the value a designer already has in a `let`. SVG's four-way `large-arc` × `sweep` choice collapses to one `"ccw"`/`"cw"` string, because a centre fixes which ellipse. Its gap is endpoints symmetric about the centre, such as the top and bottom of an oval, which leave the radii undetermined. There it falls back to a circle when the endpoints are equidistant and is an error otherwise.
- **Radii mode** is SVG's endpoint arc without rotation, and it covers that gap: a half-oval is `rx`, `ry`, and a diameter chord. It also reaches what centre mode cannot, such as a fillet of known radius between two points. Fixed radii admit two centres, one on each side of the chord, so this mode brings back SVG's large-arc flag as `large: bool`, default `false`. Unlike SVG, radii too small for the chord are an error rather than silently enlarged.

An earlier draft let a centre-mode arc also declare one radius. It was dropped because a declared radius beside a centre is redundant in the common case and needs an endpoint-on-ellipse check to catch disagreement. The two modes need that check only at their fallback edges. A rotated ellipse is not expressible as an arc — split it, or use `cube`.

**The one positional rule is reflection**, SVG's `S`/`T`: a `cube` may omit `c1` when the previous segment is a `cube`, which then takes the reflection of that cube's `c2` through the joint. `quad` does the same with `c`. It keeps a smooth chain of curves from repeating every mirrored handle. It applies only after a segment of the same kind — SVG's fallback to the current point after any other command is a silent degenerate handle, so here it is a structural error instead.

#### Why not Hobby splines

The earlier grammar was METAFONT's: a `spline` segment took `dir`/`fromDir` tangents, `tension`, and end `curl`, and Hobby's algorithm solved for any direction left free. It was dropped:

- **Non-local geometry.** A free direction is a tridiagonal (cyclic, on a closed path) solve over the whole run, so moving one point reshapes its neighbours' curves. Every explicit segment depends only on itself, and on its predecessor under reflection.
- **An implicit inheritance rule.** A segment's departure direction came from the previous declaration's `dir`, unless a `line` intervened or `fromDir` overrode it. That rule was the grammar's one regression, and needed its own tests.
- **An external oracle.** Verifying Hobby's curve needs METAFONT as a differential oracle. Explicit segments need only Bézier and ellipse identities.
- **Editor mapping.** Control points are the handles every drawing tool already shows; `tension` and `curl` have no direct-manipulation equivalent.

The cost is authoring effort: smooth joints and handle lengths are written, not computed. Arcs absorb most of it — nearly every letterform curve that meets axis-aligned tangents is a quarter ellipse — and `polar` places the remaining handles at a named angle and length.

#### Naming and closing

**Naming a segment names its endpoint.** `arc shoulder (center: c, to: r, sweep: "cw")` makes `shoulder` the key for `joinAt` (§7.3) — and "join at `shoulder`" reads correctly, since joins occur at points. Names are genuinely optional: `joinAt` is the only thing that needs them, so most segments in practice have none. Names may not be reserved words (§6.5).

**`close` is a body declaration, last in the chain, not a config field.** It *is* the closing segment — a straight line from the last endpoint back to the start's point, SVG's `Z` — so it is written where it actually occurs rather than as a header flag that silently appends one. A curved closure is written as an ordinary final segment ending at the start point.

A path is closed if and only if its body declares `close`. Coincident endpoints alone do not close a path: that keeps the flag and the geometry from ever disagreeing, and it leaves a shape whose two capped ends happen to meet still expressible. Where the final declaration already ends at the start point, the closing segment is degenerate and omitted, but the path is still closed.

Structural errors (§14): no `start`, more than one `start`, a field the declaration's kind does not admit, an omitted `c1`/`c` after a segment of another kind, a `joinAt` key naming no segment in the path, more than one `close`, and any declaration following `close`.

### 7.3 Strokes, stroke width, caps, joins

A rendered shape is **a path plus a `stroke`** — SVG semantics exactly, equivalently a circular pen of that diameter. There is no pen object, no pen angle, no razor. Stroke width, caps, and joins are properties of the path (§7.2); nothing is global.

```
path bowl (stroke: stem) { … close }         // close: no caps

path spine (
  stroke: stem,
  caps:  ("butt", "round"),                           // asymmetric ends: (start, end)
) { … }

path arm (
  stroke:  stem,
  joins:  "miter",                                  // path-wide default
  joinAt: { elbow: "bevel", wrist: "round" },         // per-point overrides
) {
  start      (at: a)
  line elbow (to: b)
  line wrist (to: c)
  cube       (c1: polar(c, k, 0deg), c2: polar(d, k, 90deg), to: d)
}
```

`joinAt` is **keyed by segment name**, which names that segment's endpoint (§7.2.1). It is the only field that needs segment names at all.

#### Caps and joins — the SVG sets, as string literals

Both are small closed sets with **no arguments**, defined over the two side-boundary endpoints `L = p(end) + r·n̂` and `R = p(end) − r·n̂` that the offset walk already holds. None requires an intersection except `"miter"`.

| Cap | Definition | Cost |
|---|---|---|
| `"butt"` (default) | The straight chord `L`→`R`, perpendicular to the tangent. | Zero. |
| `"round"` | Semicircular arc `L`→`R`. | Zero — one arc. |
| `"square"` | Extend the skeleton by `r` along the outward tangent, then `"butt"`. | Zero — path extension plus `"butt"`. |

| Join | Definition | Cost |
|---|---|---|
| `"miter"` (default) | Extend both offset boundaries to their intersection; fall back to `"bevel"` past the limit. | One local curve–curve solve. |
| `"round"` | Arc between the two boundary endpoints. | Zero. |
| `"bevel"` | Straight segment between them. | Zero. |

The **miter limit is a named implementation constant** (4, the SVG default), not a language argument — keeping both sets uniform and the DSL surface small. Promote it later only if a real design needs per-join control.

Since the implicit pen is a circle, there is no "untreated envelope" case to name: a circular pen's own boundary at an endpoint *is* a semicircle, so what would have been `natural` is simply `"round"`. That is why SVG never needed the concept, and why it is absent here.

#### Angled terminals come from the final segment's last control point

A `"butt"` cap is perpendicular to the tangent, and that tangent is yours to set: it points from the final segment's last control point to its endpoint. So the cut angle is fully controllable — not by a cap parameter, but by the segment:

```
path arm (stroke: hairline, caps: "butt") {
  start (at: b)
  cube  (c1: polar(b, k, 0deg), c2: polar(t, k, 70deg + 180deg), to: t)   // butt cap lands at 70deg + 90deg
}
```

Writing that control point as `polar(end, len, θ)` is therefore the terminal-angle tool. Tie `θ` to a parameter and every terminal in the face rotates together.

**The coupling this imposes**, stated plainly because it is the accepted cost of having no region booleans (§1.1): the cut angle and the stroke's direction at that point are the same degree of freedom. A curved arm that must arrive at 70° but be cut at 90° cannot be built directly. End the skeleton with a short straight segment oriented to give the cut you want, and accept a slight corner where it meets the curve — real terminals frequently have one. A tangent-continuous curve with an off-perpendicular cut is not expressible.

#### Flat tops and apexes come from an overlapping path

The other thing `trim` used to do — cut flat across several strokes at once — is an extra path:

```
glyph A (codepoint: U+0041) {
  let apexY = capHeight.y - stem/2;

  path legL (stroke: stem) {
    start (at: (xL, apexY))
    line  (to: footL)
  }
  path legR (stroke: stem) {
    start (at: (xR, apexY))
    line  (to: footR)
  }
  path apexBar (stroke: stem) {   // overlaps both legs, fusing the apex flat
    start (at: (xL, apexY))
    line  (to: (xR, apexY))
  }
}
```

Both legs end below cap height; the bar spans them at the top. Overlap does the rest at zero cost (§9.1). The same trick gives flat tops on `t`, `f`, `E`, and flat feet anywhere a slab would have been cut.

#### `stroke` is one number per path

`stroke` is a single `num` in the path's config, constant along the whole path. There are no per-segment overrides, no profiles, and no variable width anywhere in the language.

```
path upright (stroke: stem) {
  start (at: (sx, top))
  line  (to: (sx, 0))
}
```

Two consequences follow, and both are worth stating in full because they set the boundary of what typefaces this system can build.

**What this deletes.** The variable-width envelope (§8) disappears. That mattered: a stroke of varying radius is not a normal-direction offset but the envelope of a family of circles, touching each at an angle off the normal where `sin φ = −r′(t)/|p′(t)|`, with its own degeneracy when the width grows faster than the centre point moves. It was the single subtlest correctness trap in the geometry kernel and it is now simply not a case. Continuity rules, width-key errors, and the ramped-width differential test all go with it.

**What this costs — contrast within a *stroke* is not expressible.** Contrast *between* strokes is fully supported and covers a great deal: a stem at `stem` and a crossbar at `hair` are separate paths, so `E`, `A`, `7`, `4` and every bar-and-stem letter works exactly as intended.

A high-contrast bowl — thick at the sides, hairline at top and bottom — cannot be one stroke. Splitting it into four constant-width arcs produces a **visible step at each join**, since the arcs' widths differ and the boundary jumps by half the difference on each side. At text sizes that may pass; at display sizes it will not.

**`fill` is the answer to that, and it is a different bargain rather than a fix.** Such a bowl is authored as a filled closed path: one contour, no steps, any modulation you like. What it forfeits is the parametric guarantee — a stroked skeleton follows `stem` automatically, a filled outline follows it only where its coordinates are written as expressions. So the honest scope is: **monolinear and low-contrast designs are served by stroking, which is the mode the system reasons about; high-contrast shapes are expressible as fills, at the cost of hand-maintaining their parametric behaviour.** Variable width and §8's envelope remain out, and `fill` is why they can stay out.

### 7.4 Region assembly

```
glyph I (codepoint: U+0049,
         advance: glyph.bbox.width + 2 * sidebearing) {
  path upright (stroke: stem)       { … }
  path foot    (stroke: serifThick) { … }   // slab foot: another path
  path head    (stroke: serifThick) { … }   // and its mirror
}
```

A glyph renders every path that declares `stroke` or `fill`, in declaration order. Overlap between them is fine and expected (§9.1).

**A slab serif is a second path.** A cap can only *close* a boundary — it cannot add width, and a slab is wider than the stem it sits on. So serifs are short paths with their own width, overlapping the stem. Under the always-keep-overlaps decision this costs nothing.

**Counters arise naturally, three ways.** Stroking a *closed* path yields two contours — outer and inner offset — so the bowl of `o` produces its counter with no hole construction at all. A filled path nested inside another filled path is the third (§9.3).

Less obviously, **counters also work when they are enclosed between two separate overlapping open paths** — the stem and bowl of `B`, or the spine and bowl of `6`. Neither contour encloses the counter, so under nonzero winding it is simply uncovered and no inner contour has to be constructed. This is the mechanism that makes "no region operators" survivable for letterforms with attached bowls, and it is worth stating because it looks like it should need a boolean and does not (Appendix A.1).

### 7.5 A complete worked example

Every construct in §5–§7 in one file, to serve as the reference for the grammar plan (§17.1):

```
font (name: "Metaglyph Sans", em: 1000)

param stem        (default: 100,  range: 20..300)
param contrast    (default: 0.35, range: 0.05..1)
param sidebearing (default: 40,   range: 0..120)

let hairline = stem * contrast;        // top-level let, referenced bare

metric baseline  (y: 0,   overshoot: 10, align: "bottom")
metric xHeight   (y: 520, overshoot: 10)
metric capHeight (y: 700, overshoot: 12)
metric ascender  (y: 740)
metric descender (y: -220, align: "bottom")

glyph o (codepoint: U+006F,
         advance: glyph.bbox.width + 2 * sidebearing) {

  let w = stem * 4.2;                  // centreline extent, not ink width
                                       //   (§13.1 — ink is w + stem)
  let ctr = (w/2, xHeight.y / 2);      // the bowl's centre
  path bowl (stroke: stem) {
    start (at: (w/2, xHeight.ink))                         // .ink adds overshoot
    arc   (center: ctr, to: (w,   xHeight.y / 2), sweep: "cw")
    arc   (center: ctr, to: (w/2, baseline.ink),  sweep: "cw")
    arc   (center: ctr, to: (0,   xHeight.y / 2), sweep: "cw")
    arc   (center: ctr, to: (w/2, xHeight.ink),   sweep: "cw")
    close
  }
}

glyph A (codepoint: U+0041,
         advance: glyph.bbox.width + 2 * sidebearing) {

  let w     = stem * 6;
  let apex  = (w/2, capHeight.ink);
  let footL = (0, 0);
  let footR = (w, 0);
  let barY  = capHeight.y * 0.42;

  // meet() determines these outright — no partial facts, no free variables
  let barL = meet(lineThrough(apex, footL), hline(barY));
  let barR = meet(lineThrough(apex, footR), hline(barY));

  // legs stop short of cap height; apexBar overlaps them to fuse a flat apex
  let apexY = capHeight.y - stem/2;

  path legL (stroke: stem) {
    start (at: (w/2 - stem/2, apexY))
    line  (to: footL)
  }
  path legR (stroke: hairline) {
    start (at: (w/2 + stem/2, apexY))
    line  (to: footR)
  }
  path apexBar (stroke: stem) {
    start (at: (w/2 - stem/2, apexY))
    line  (to: (w/2 + stem/2, apexY))
  }
  path bar (stroke: hairline) {
    start (at: barL)
    line  (to: barR)
  }
}

instance Regular (stem: 100, contrast: 0.35)
instance Bold    (stem: 180, contrast: 0.30)
```

Two things worth noticing. Every value's determination lives in its own statement — there is nowhere else to look. And the config/body split puts a glyph's identity and metrics on its first line, so scanning a file tells you what it contains before you read any geometry.

---

## 8. Offset generation — the core geometry algorithm

With SVG stroke semantics the pen is always a circle, and with width constant per path (§7.3) there is exactly **one** case: approximate the normal offset and fit cubics (§8.1). No support functions, no convexity, no envelope of a circle family.

**Why no custom pens.** A pen-based stroker is the more powerful model and it has a genuinely elegant core: for a convex *polygonal* pen the envelope is exact, consisting of translated copies of the path segments joined by pen edges, and an elliptical pen reduces to a circular one because affine maps commute with the Minkowski sum. Rejecting it costs the free calligraphic contrast model — an angled elliptical pen gives thick verticals and thin horizontals for nothing.

It is rejected anyway because a circular pen deletes the entire support-function apparatus, convexity validation, razor pens, and the `natural` cap and join, while contrast between strokes remains fully expressible (§7.3). The part a pen would have given for free and this does not is contrast *within* a stroke — see §7.3 for that boundary, which is the real cost of the pair of decisions.

### 8.1 Offset curve approximation

`stroke` is one constant per path (§7.3), so `r = stroke/2` is constant and the boundary is exactly `p(t) ± r·n̂(t)`. This is the **only** offsetting case — there is no variable-width envelope, no canal curve, no tilt of the offset direction away from the normal.

There is no exact cubic Bézier offset of a cubic Bézier, so this remains the single approximation in the geometry pipeline and the place to spend care. The mandated method:

1. Split the source curve at **inflection points and curvature extrema**, and wherever curvature radius falls below `r` (cusp locations).
2. For each piece, construct a candidate offset cubic by matching endpoints, endpoint tangents, and one interior point.
3. Measure error by sampling the true offset against the candidate; if max deviation exceeds tolerance, subdivide and recurse.
4. Tolerance: a fraction of a design unit (e.g. 0.05 units at 1000 upem) — well below quantization, so the approximation is invisible in output.

Reject Tiller–Hanson (offsetting the control polygon legs) as the primary method: it is fast but degrades badly at high curvature, exactly where letterform joins live. It is acceptable as a first guess feeding step 3.

### 8.2 Cusps and degeneracies

Where the path's curvature radius is smaller than `r` on the inner side, the inner offset folds back and self-intersects. Detect this **analytically, before generating geometry** — compare `r` against the curvature radius along the path, which needs no intersection code.

Per §9.2's recommendation this is an **error**, not a warning: a stroke wider than twice its curve's radius is a broken letterform, and erroring here is what lets §9 drop self-intersection resolution entirely. Report the glyph, the path, the parameter interval, and the instance at which it begins — `stroke` is swept across instances, so the same source may pass at Regular and fail at Bold, and that is precisely what the designer needs told.

### 8.3 Caps and joins are part of the offset walk, not a post-pass

This is why caps and joins cost almost nothing (§1.1).

Offset generation walks the path once producing the left boundary, once producing the right boundary, and then **closes the two ends. The cap *is* that closing.** Because the cap set is the parameterless SVG trio (§7.3), every cap is a construction on the two side-boundary endpoints the walk already holds:

- `butt` — one straight segment `L`→`R`.
- `round` — one semicircular arc `L`→`R`.
- `square` — extend the skeleton before the walk, then `butt`. No new code in the closer at all.

**Zero intersections, zero region representation, no sweep line, no winding classification.** Caps and joins must therefore live *inside* the offset generator. This is not merely an optimization: with no region operators in the language (§1.1), there is no region layer to build them on top of.

Joins are the same shape of operation at an interior point: `bevel` is one segment, `round` is one arc, and `miter` needs a single curve–curve solve between two *known adjacent* boundary pieces, plus the limit check with fallback to `bevel`. The **inner side of every corner** needs one more solve of the same kind: the two inner boundary pieces are cut at their crossing nearest the corner and meet there (spec §7.4). These are the only intersections in the cap/join system. Both are local, between two known pieces, never a search.

**Degenerate cases to specify:**

- Zero-length stroke (endpoints coincident) → both boundaries degenerate; geometry error rather than a zero-area contour.
- Zero or negative `stroke` → geometry error. A `stroke` of 0 collapses the two boundaries onto the skeleton; a negative one is meaningless.
- `square` on a path whose extension would invert a segment shorter than the extension amount → extend the *boundary* endpoints directly rather than re-walking a malformed skeleton, so the treatment stays local.
- `miter` at a point where the two boundary pieces are very nearly collinear → the intersection is ill-conditioned and shoots far away. The limit check must be evaluated *before* trusting the intersection, not after.

---

## 9. Curvature validation and winding

### 9.1 Required vs. optional

With `trim`, `union`, and `difference` gone from the language (§1.1), four things remain in this stage.

- **Free — caps and joins.** Handled inside the offset walk (§8.3). Local, no region representation involved.
- **Overlaps are kept, permanently.** Modern rasterizers (FreeType, CoreText, DirectWrite) fill `glyf` and CFF outlines with the **nonzero winding rule**, so overlapping contours of consistent direction render identically to their union. Many production fonts ship with overlaps.
- **Self-intersection within one stroke** — decided in §9.2.
- **Self-intersection within a filled contour** — not removable by any analytic precondition, because a fill's outline is authored directly rather than derived from a skeleton and a radius. See below.

**Do not over-credit the absence of region operators.** Having none removes the region-boolean API, arbitrary-input robustness, and half-plane clipping. It does **not** by itself remove the intersection-plus-winding primitive, because a single self-overlapping contour still needs it — the same algorithm on bounded input, not different code. Stated explicitly so the geometry kernel plan is not scoped optimistically; §9.2 is what removes it for strokes.

**A consequence to accept deliberately:** there is now no `--remove-overlap` export flag, because removing overlaps *is* a union. Overlapping contours are what ships. The costs are that TrueType hinting behaves slightly better without overlaps, and that some old or unusual downstream tools mishandle them. Both are acceptable given §12.3 already declines to hand-roll TrueType instructions.

### 9.2 Decision — tight curvature is an error, not resolved

A stroke's offset folds back when `stroke` exceeds twice the curvature radius on the inner side (§8.2). The choice was between resolving that loop in this stage and refusing it.

**A correction to an earlier claim.** Curvature is not the only way a stroke outline overlaps itself. A skeleton that crosses itself, and two non-adjacent parts of one path passing within `stroke` of each other, both produce self-overlapping outlines regardless of curvature. Those are benign: filled with nonzero winding, the outline renders exactly the stroke's ink, so the spec ships them as-is (spec §7.4). The curvature error is therefore a design judgement that a folded curve is a broken letterform, not a precondition for correct rendering.

**Inner corners are the exception, and are resolved.** The inner side of a corner used to be in the benign list too. It is now trimmed: the two inner boundary pieces are cut at their crossing nearest the corner, so each path is one clean outline with no loop inside its turns. This is cheap because it is local, one curve–curve solve between two known adjacent pieces, like `miter` (§8.3). It is not overlap removal. When the crossing falls outside the two adjacent segments (a sharp turn beside a segment too short for its stroke), or does not exist (an exact 180° reversal), it is an error rather than a search across segments, on the same judgement as the curvature error: a stroke that swallows a whole segment is a broken letterform.

The alternative is to **detect the condition analytically before generating any geometry and make it an error.** The check is cheap — compare `stroke/2` against the curvature radius along the path — and it needs no intersection code at all.

**For strokes that is the whole story; for fills it is not.** A filled contour has no radius to compare against, so no analytic precondition rules out self-intersection — a figure-eight outline fills its two lobes with opposite winding and one of them vanishes. That is an **error** too, but detecting it genuinely needs a curve–curve self-intersection test over the contour's own segments. So the primitive returns, in its cheapest form: a detector on bounded input, never a resolver, and only on paths that declare `fill`. Resolution stays out of the system, because resolution is a union.

The honest accounting: `fill` costs one curve–curve test that the stroke-only design had eliminated. It is the same primitive `intersect` already needs (§6.3), on small inputs, with no winding classification and no planar arrangement behind it.

Arguments for erroring, which is the consistent choice with the rest of the spec:

- A stroke whose width exceeds its curve's radius is a **broken letterform**, not a shape to be quietly repaired. The resolved output looks pinched regardless.
- It matches §5's precedent that underdetermination is an error, not a warning, and the general preference in this design for failing loudly over silently fixing.
- It keeps intersection code out of the *stroke* path entirely. The detector fills need (§9.2) is bounded, optional per path, and never resolves anything, so the robustness risk stays small.

The argument against: `stroke` is a swept parameter. A design that is fine at Regular may trip the condition at Bold on one tight curve, and a font that *fails to build* at Bold is worse than one with a cleaned-up curve. Mitigation would be to report it as a per-instance build error naming the glyph, the path, and the parameter value at which it begins — actionable, and arguably information the designer needs anyway.

**Decision: error.** It is a reversible decision with a real trade, so it is recorded here rather than folded silently into §1. If real designs trip it too often, the fallback is to implement resolution — and the interface between the two is just "what §8.3 does when it detects the condition."

### 9.3 Contour roles and winding direction

Export must normalize, because the two formats disagree:

- **TrueType `glyf`:** outer contours clockwise, counters counter-clockwise (y-up).
- **PostScript / CFF:** outer contours counter-clockwise, counters clockwise.

**Roles come from the producer, not from global nesting.** A stroked open path gives one outer contour; a stroked closed path gives an outer and a counter, and the offset walk already knows which boundary is which; a filled path gives one outer contour. Nesting inference by point-in-contour applies **only among filled contours**, which is how a fill inside a fill becomes a hole.

Deriving roles from nesting depth across all contours would be wrong once `fill` exists. A closed path carrying both `stroke` and `fill` produces three nested contours — stroke-outer, fill, stroke-counter — and depth parity would make the fill a counter, rendering a ring with a hole instead of a solid shape grown by `stroke/2`. Carrying roles from the source gives `+1` throughout under nonzero winding, which is correct and needs no special case.

Then compute signed area per contour and reverse where direction disagrees with role. The conventions above are correct; they are frequently stated backwards elsewhere. Under nonzero winding a uniform reversal of every contour still renders correctly — what matters for rendering is the relative direction of outer and counter, and the absolute convention matters for hinting and downstream tools.

---

## 10. Editor tool inventory and text-canonical editing

### 10.1 Tools (each maps to a named structured edit on the syntax tree)

**Construction:** point (literal or derived via the §6.3 constructors), line (`lineThrough` / `lineAt` / `hline` / `vline`), metric guide, vertical guide, symmetry axis, grid, measurement. Note there is no "constraint" tool — in a directed system the equivalent tools *rewrite definitions*, covered below.

**Shape:** path tool (place points, which appends `line`, `quad`, `cube`, or `arc` segments), **fill toggle** (sets `fill`, appending `close` if the path lacks it), **segment-kind toggle** (among `line`, `quad`, `cube`, `arc` — and since `line` admits no control points, switching kinds *removes* fields rather than leaving dead ones behind), **control-point handles** (`quad`, `cube`) and **centre and sweep controls** (`arc`), **width tool** (set `stroke`, which is also what promotes a construction path to a rendering one), **cap tool** (per-end; three-way pick `"butt"` / `"round"` / `"square"`), **join tool** (three-way pick `"miter"` / `"round"` / `"bevel"`, path-wide default plus per-segment override), transform & instance placement, composite reference.

Segment-typed declarations pay off directly here: the editor never has to gray out an inapplicable field, because an inapplicable field does not exist on that declaration (§7.2.1).

Because `path` is the only shape construct (§7.2), every one of these tools sets a property on a single block — there is no "bind this path to a stroke" step for the editor to model, and no separate render list to keep in sync.

Because caps and joins are parameterless string values (§7.3), those two tools are pure three-way toggles — no numeric fields, no drag handles. The two tools that carry real expressive weight are instead:

- **The end-direction tool**, which is the terminal-angle tool (§7.3). It rewrites the final segment's last control point as `polar(end, len, θ)`, which rotates the butt cap; tie `θ` to a parameter and every terminal in the face turns together. There is no trim tool, because there is no trim (§1.1).
- **The stroke tool**, which is the contrast model (§7.3). Dragging a stroke width is the most direct expression of design intent in the whole editor, and it is a genuine scalar drag — so it is the one place where case 1 of §10.2 (rewrite a literal) always applies cleanly. Since `stroke` lives on the path, the drag target and the text node are the same thing.

**Relationship:** these tools **rewrite one definition**, which is the whole framing difference from CAD. There is no constraint to add, and no solver to satisfy — the edit is a source transform, and its result is visible immediately because the graph re-evaluates.

| Tool | Rewrites as |
|---|---|
| Make coincident | one point's definition becomes a reference to the other |
| Snap to intersection | `let p = meet(lineThrough(a,b), lineThrough(c,d));` |
| Project onto line | `let p = project(q, l);` |
| Place at fraction | `let p = mediate(a, b, 0.35);` |
| Make parallel / at angle | `let p = polar(q, len, θ);` |
| Mirror across axis | `let p = mirror(q, axis);` |
| Promote literal to parameter | a literal becomes a reference to a new or existing `param` |

Plus: make-smooth on a joint (the tangency tool — omits `c1`/`c` so it reflects the previous control, or rewrites it as `2·p − c′` where reflection does not apply), and the raw expression editor.

Because every tool's output is one of the §6.3 constructors, **the tool inventory and the construction library are the same list**. That is a useful check in both directions: a tool with no constructor to emit is a gap in §6.3, and a constructor no tool emits is probably dead weight given the decision to start minimal.

**Diagnostics:** dependency inspector (upstream and downstream of any value — trivial here, since the graph *is* the program), cycle reporter (§5.3), unresolved-name reporter, parameter sweep preview (drag `stem`, watch the whole font rebuild).

### 10.2 Inverse drag — the critical UX mechanism

With a text-canonical directed model, dragging cannot universally "just work," because a dragged point may be derived. Define three cases:

1. **Free point** (literal coordinates) → rewrite the literals. Trivial.
2. **Point derived from exactly one numeric literal or parameter upstream** → solve the scalar inverse numerically (secant/Newton on that one variable, bracketed by its declared range) and rewrite that number. The glyph re-evaluates through its full dependency chain, so all sibling geometry follows. This is what makes the editor feel direct rather than like a text editor with a preview.
3. **Point with multiple or zero upstream literals** → present its literal ancestors and ask which to drive, then apply case 2. A "last driven" memory per point makes repeat drags immediate.

This mechanism deserves its own implementation plan section; it is the difference between a pleasant tool and a compiler with a picture window.

### 10.3 Round-trip requirements

- **Lossless syntax tree:** comments, blank lines, and original formatting survive any structured edit. Only touched nodes are reformatted.
- **Stable node identity:** selection, undo, and diagnostic anchors must survive re-parse. Use stable IDs assigned at parse and preserved across edits, not byte offsets.
- **Incremental evaluation:** an edit dirties a subgraph; re-evaluate only affected glyphs. Per-glyph scoping (§5.5) is what makes this tractable.
- **Partial-text tolerance:** while the user is typing, the source is often invalid. The evaluator must render the last good state plus a diagnostic, never a blank canvas.

---

## 11. Compilation to outlines

### 11.1 Glyph-level assembly and component reuse

Component reuse is **first-class**, since it is the chosen mechanism for accents and derived glyphs in place of anchor-based mark attachment:

```
glyph eacute (codepoint: U+00E9) {
  component (glyph: e)
  component (glyph: acute,
             offset: (glyphs.e.top.x - glyphs.acute.top.x,
                      accentGap))
}                                 // no paths of its own — only components
```

`component` config is exactly: **`glyph:`** (required), plus **either `offset: <pair>` or `transform: <transform>`** — never both. `offset` is the common case and is sugar for `transform: translate(…)`; `transform` covers rotation and reflection, as when `9` is a rotated `6`.

Placement may be a **parametric expression** over the referenced glyph's measured geometry, so accents re-centre automatically when weight changes. What is readable from another glyph is a fixed, deliberately narrow set (§6.5): **`advance`, `bbox`, and declared `anchor`s** — never its `let`s or paths, which are that glyph's internal business. Component graphs must be acyclic; depth is limited (recommend 5) and cycles are an export error.

Export either as real `glyf` composites (smaller, preserves the relationship) or decomposed. Decomposition is **required** when the component carries a transform `glyf` cannot express (non-uniform scale with rotation beyond the F2Dot14 2×2 range), and always for CFF output, which has no equivalent nesting for arbitrary transforms.

Optional-but-cheap addition worth noting for the implementation plan: a named `anchor` declaration per glyph (`anchor top (at: (w/2, capHeight.y))`) serves both parametric component placement *and*, later, GPOS mark attachment if complex-script support is ever added. Declaring anchors now costs almost nothing and avoids a retrofit — and it is what makes `glyphs.e.top` in the example above resolve (§6.5).

### 11.2 Extrema insertion and quantization

1. **Insert on-curve points at horizontal and vertical extrema** of every contour. Required for sane hinting and expected by several rasterizers and validators.
2. **Snap to metric zones before rounding**: points within a small tolerance of a declared metric guide (baseline, x-height, cap-height, overshoot lines) round *to that guide's integer value*, not to the nearest integer independently. Prevents a 519.6 x-height point and a 520.4 one landing on different pixels.
3. **Quantize** remaining coordinates to integers with a specified rule (round half away from zero). Re-verify that rounding did not introduce a new self-intersection or reverse a tiny segment's direction.

### 11.3 Curve conversion per format

- **CFF / CFF2 (`.otf`)** — cubics pass through directly. Charstring encoding, optional subroutinization for size.
- **TrueType `glyf` (`.ttf`)** — quadratic B-splines only, converted from unrounded cubics before quantization (§2). Use the **cu2qu** approach: recursively split each cubic until a single quadratic (or a short quadratic spline) approximates it within tolerance; then exploit TrueType's *implied on-curve points* (a midpoint between consecutive off-curve points is implicit) to collapse the point count. Tolerance ~0.5 design unit at 1000 upem is the industry norm.
- **WOFF2** — not a new outline format: `glyf`/`loca` are preprocessed into WOFF2's transformed representation and the whole table directory is Brotli-compressed. Purely a repackaging step after TTF assembly.

### 11.4 Limits to validate

`maxp` point and contour counts are `uint16` — 65535 points per glyph. Coordinate range in `glyf` is int16 per delta. Check both, plus total table sizes, and report as export errors rather than producing a corrupt font.

### 11.5 Codepoints, glyph names, and `cmap`

Arbitrary Unicode is in scope, so this cannot be a Latin-only shortcut:

- **Glyph identity is the DSL name**, not the codepoint. A glyph may carry zero codepoints (components, alternates) or several. Names must be valid, stable identifiers — reject names that collide after case-folding, and generate `.notdef` rather than accept it from source. The spec restricts glyph names to the identifier grammar, with no `.`, so every glyph name is writable in source. Production names such as `a.sc` would need the lexer to admit dots, which collides with member access (`glyphs.a.sc.advance`); a quoted-name form is the likely answer if that becomes necessary. Glyph ID 0 is always `.notdef` and must be generated if not declared.
- **`cmap` subtables:** format 4 for the BMP, plus **format 12 whenever any codepoint exceeds U+FFFF** (supplementary planes). Emit both; format 4 alone silently drops astral codepoints. Platform/encoding records: (3,1) for format 4 and (3,10) for format 12; a (0,3)/(0,4) Unicode-platform pair is good hygiene.
- **Validation:** a glyph's `codepoint` field takes plain integers (§3), so it is range-checked to 0–0x10FFFF as a field rule; duplicate codepoint across glyphs is an error; surrogate-range and noncharacter codepoints are an error; unassigned codepoints are a warning, not an error (private-use and provisional designs are legitimate).
- **`post` table:** version 2.0 for TTF carries the glyph names, which is what makes the output debuggable in other tools. Names over 63 bytes are invalid — check.

---

## 12. Hinting — where this architecture wins

Autohinters reverse-engineer intent from outlines: *is this pair of edges a stem? is this y-coordinate an alignment zone?* For a stroked path both are **declared** in this system, and hinting becomes a lookup.

The exception is `fill`. A filled outline declares no stem, so there is no pair of offset lines to read a hint from and §12.3's derivation has nothing to work with — a glyph built from fills gets alignment zones (which come from `metric`, not geometry) and no stem hints. That is the third cost of `fill`, after the parametric one (§7.3) and the self-intersection detector (§9.2).

### 12.1 Alignment zones from metric guides

Every `metric` declaration is an alignment zone, and its `align` field says which kind (§7.1). Emit directly:

- **CFF:** `align: "top"` zones become **`BlueValues`** entries, as the pair `(y, y + overshoot)`. `align: "bottom"` zones become **`OtherBlues`** entries, as `(y − overshoot, y)` — except `baseline`, whose zone must be the first `BlueValues` pair. Plus `BlueFuzz`, `BlueScale`, `BlueShift`. This mapping is precisely why `align` is declared rather than inferred — for a free-form zone the exporter has no other way to tell the two tables apart.
- **TrueType:** no equivalent without glyph instructions (§12.3).

Note the limit: CFF allows 14 `BlueValues` entries (7 zones) and 10 `OtherBlues` (5 zones). A face declaring more metrics than that must have the excess dropped, in a defined order, with a warning — rather than producing an invalid Private DICT.

### 12.2 Stem widths from parameters

`stem`, `hairline`, `serifThick` and friends are named numbers. Emit `StdHW` / `StdVW` from the dominant horizontal/vertical stem parameters and `StemSnapH` / `StemSnapV` from the full sorted set. No clustering heuristics needed.

### 12.3 Per-glyph hints

- **CFF:** emit `hstem`/`vstem` hints directly from the stroke's construction — the two edges of a stem stroke are known symbolically, so the hint pair is the envelope's two offset lines. Hint replacement (`hintmask`) where stems overlap in the other axis. Filled paths contribute none.
- **TrueType:** do not hand-roll a TT instruction generator. `cvt` and `prep` alone cannot move glyph points — zone rounding needs per-glyph programs — so the spec ships TrueType unhinted with a `gasp` table selecting grayscale/subpixel rendering at all sizes. The realistic upgrade is a ttfautohint-equivalent pass over the finished outlines, which needs nothing from the language. Full TrueType hinting is a separate project and modern rendering stacks barely reward it.

This §12 advantage is worth stating loudly in any project write-up: it is the clearest concrete payoff of the parametric approach beyond design-time convenience.

---

## 13. Metrics and kerning

### 13.1 Advance widths and sidebearings

#### Centreline extent versus ink extent

This distinction has to be settled explicitly, because paths are **centrelines** (§7.3) and it is easy to space a whole font wrong by forgetting it.

If a glyph's paths span centreline `x ∈ [0, w]` with width `stem`, its **ink** spans `[−stem/2, w + stem/2]`. So:

- `advance: w + 2 * sidebear` measures bearings from centrelines. The *visible* sidebearing is `sidebear − stem/2`, and the glyph has ink at negative x, overlapping its neighbour.
- `advance: glyph.bbox.width + 2 * sidebear` measures from ink, which is what a designer means by a sidebearing.

**Use the ink model.**

#### `advance`, `lsb`, `rsb`

The first design had a single `advance` field and no automatic shift, so paths had to be authored in final position: the leftmost ink at `x = sidebear`, which for a stem starting the glyph means its centreline sits at `sidebear + stem/2`. The right bearing came from `advance: glyph.bbox.x1 + sidebear`.

That cannot centre ink in a fixed advance, which is what every glyph of a monospace font needs. A glyph's horizontal spacing comes down to two numbers: the advance and a horizontal shift of the ink. The ink width is measured, so it is not a field. The spec (§12.1) therefore takes three optional fields, `advance`, `lsb`, `rsb`, of which a glyph declares one or two:

```
glyph E (…, rsb: sidebear)                                  // authored in place, as before
glyph o (…, lsb: sidebear)                                  // equal bearings
glyph A (…, advance: cell, lsb: (cell - glyph.bbox.width) / 2)   // monospace, centred
```

A monospace `auto` value for centring was considered and rejected. The explicit expression is longer, but it adds no keyword.

**The editor objection.** An automatic shift was first rejected because it adds a second transform between source and output coordinates, which the editor would have to invert on every drag and every displayed coordinate. The resolution is to never show output coordinates for the glyph being edited. The canvas stays in authored coordinates and draws the origin and advance guides at `−shift` and `advance − shift`. The ink stays put and the frame moves, so a drag still rewrites an authored literal directly. Other glyphs are read in placed coordinates (`glyphs.X.bbox`, anchors, components), so accent placement is unaffected.

`glyph.bbox` is the bounds of all ink from this glyph's rendering paths (§6.5). This is the one place the language needs a name for "all of it," because spacing is inherently about the whole glyph rather than any one path.

Optical sidebearings need `leftEdgeAt` / `rightEdgeAt`, which §6.3 defers until a design actually calls for them:

```
let lsb = spacingUnit
        - glyph.leftEdgeAt(xHeight.y / 2) * 0.5;
```

Both explicit and outline-derived (optical) sidebearings must be expressible.

**Vertical metrics come from the five reserved metric names** (§7.1): `hhea.ascent`/`descent` and `OS/2.sTypoAscender`/`sTypoDescender` from `ascender` and `descender`; `OS/2.sCapHeight` from `capHeight`; `OS/2.sxHeight` from `xHeight`; the origin from `baseline`. `usWinAscent`/`usWinDescent` come from the union of all glyph bounding boxes rather than from a metric, since they must clip nothing. This is why those five names are reserved rather than free-form — the exporter needs a specific zone, not a set of them.

### 13.2 Kerning

Parametric class-based kerning in the source:

```
group roundRight (glyphs: [ o, c, e, b, p, thorn ])
group roundLeft  (glyphs: [ o, c, e, d, q ])

kern (left: roundRight, right: roundLeft, by: -0.015em)
kern (left: A,          right: V,         by: -0.05em * kernStrength)
```

Kerning is parametric because `by:` is an ordinary expression — `kernStrength` is a `param` and sweeps per instance like any other.

Compile to GPOS lookup type 2 (pair adjustment), format 2 (class-based) for group pairs and format 1 for singletons. Keep classes as classes — do not expand to pairs — or the table explodes.

**Kerning is the only OpenType layout the language expresses**, so GPOS is the only layout table generated. Every block in the grammar holds declarations or a map literal, never opaque text — which is what keeps §10.3's tree-node-per-rendered-thing property total rather than partial, and keeps the lexer to a single mode.

The cost is that substitution is out of scope: `liga`, `dlig`, `smcp`, `c2sc`, figure styles, `frac`, `sups`/`subs`, `ss01`–`ss20`, `salt`, `case`, and `zero` have no representation in the source, and neither does mark attachment. That narrows the target to faces whose behaviour is spacing and shape rather than substitution, which is consistent with the script-scope decision in §1.

### 13.3 Instances

```
instance Regular   (stem: 100, contrast: 0.35)
instance Bold      (stem: 180, contrast: 0.30)
instance Condensed (stem: 100, widthScale: 0.82)

instance Italic (
  stem:     100,
  contrast: 0.35,
  slant:    11deg,           // ordinary transform, applied at export
  glyphset: Italic,          // true italic letterforms where they exist
)
```

Each instance is a full independent build from the same source. Name table, `OS/2` weight/width class, and `STAT` (worth emitting even for statics, for correct family grouping in modern UIs) derive from instance declarations.

**Italic model.** Two independent mechanisms, per the §1 decision. `slant` is an ordinary affine transform — no special machinery, and it correctly feeds `post.italicAngle` and the `hhea` caret slope. Separately, a **glyph set** is a named override group supplying alternative definitions for chosen glyphs (`a`, `e`, `g`, `f`, `y`…); an instance selects one, and glyphs absent from it fall back to the default definitions. This avoids both extremes: no separate source file per italic, and no pretending a sheared roman is an italic.

---

## 14. Error model

Every error names a source location, a variable or entity, and where possible a suggested fix.

| Class | Examples |
|---|---|
| Syntax | Unexpected token; unclosed block; malformed path expression. |
| Type | `pair` where `num` expected; `path` argument to a scalar function; `.bbox` on a value that has no extent. |
| Name resolution | Unresolved identifier, with the enclosing scope named and near-miss suggestions. Duplicate definition of a name — the only way to state two facts about one value, and therefore an error rather than a conflict to solve. |
| **Cycle** | **Circular definition**, reported as the full cycle path with file and line for each hop, plus a hint at which edge to break (§5.3). This is the system's highest-value diagnostic and the one the directed design makes unavoidable. |
| Domain | `sqrt` of a negative; division by zero; `asin` out of range; `meet` on parallel lines; `intersect` returning no crossing where one was required. Names the function and the offending argument. |
| Fill | `fill: true` on a path with no `close` declaration; a **self-intersecting filled contour** (§9.2), naming the glyph, the path, and the crossing parameters. |
| Geometry | Zero-length segment; zero or negative `stroke`; **`stroke` exceeding twice the curvature radius** (§8.2, §9.2 — an error, naming the glyph, path, parameter interval, and the instance at which it begins). Constant width per path removes the whole class of width-continuity and width-rate errors that a variable-width model would need. |
| Cap / join | Zero-length stroke (endpoints coincident); `square` extension inverting a segment shorter than the extension; `miter` on near-collinear boundary pieces (ill-conditioned intersection → limit check must precede trusting it); miter past its limit (silent fallback to bevel, reportable). Each names the stroke *and* the cap or join (§8.3). Note how short this list is — the parameterless cap set (§7.3) has no cut-line, no radius, and no extension argument, so the entire class of "the cut missed / crossed twice / fell past the endpoint" failures never arises. |
| Export | Point/contour count over `maxp` limits; coordinate out of int16 range; glyph referenced by a `group` or `kern` but not defined; duplicate codepoint; component cycle. |

---

## 15. Determinism and reproducibility

Non-negotiable given that the source is meant to *be* the font.

- `f64` throughout; no fast-math or reassociation; no platform-dependent transcendental fallbacks (specify that `sin`/`cos`/etc. must come from a fixed implementation if bit-exactness across platforms is required). The spec scopes byte-identical output to one target platform: the geometry library's internals call the platform math library, and replacing them is not worth the cost.
- Deterministic topological order: break ties among ready nodes by declaration order, never by hash or discovery order. Since the engine solves nothing, there is nothing numerically fragile about the order — only reproducibility depends on it.
- Deterministic iteration order everywhere: sorted keys, never hash order.
- Specified rounding rule at quantization; specified tolerance constants as named, documented values — not scattered magic numbers.
- `head.created` / `head.modified` settable to a fixed value; deterministic table ordering and padding. Goal: byte-identical output from identical source.

---

## 16. Verification strategy

Ordered by value per unit of effort:

1. **Evaluation engine** — property test that permuting statement order within a scope yields identical results (§5.2 claims this falls out of reference-based graph construction; prove it). Cycle detection tests covering self-reference, two-node, and long cycles, asserting the reported path is the actual cycle and not merely *a* cycle. Golden tests on the `meet`/`mediate`/`project`/`polar`/`mirror` set against hand-computed geometry.
2. **Segments** — assert a `quad`'s elevated cubic is exact, and that an `arc`'s cubic pieces stay within the known 90°-piece bound of the true ellipse, by dense sampling. Test the **reflection rule** (§7.2.1) separately and explicitly, since it is the one positional rule in the path grammar: an omitted `c1`/`c` produces a tangent-continuous joint, and omitting it after a segment of another kind is a structural error.
3. **Offsets** — measure Hausdorff distance against a densely-sampled true offset `p(t) ± r·n̂(t)` and assert it is under tolerance. One case only; constant width means there is no tilt term to get wrong.
   **Caps and joins** get their own tests, since they live inside the offset walk (§8.3): assert `butt` is perpendicular to the tangent; assert `square` equals `butt` on a skeleton pre-extended by hand; assert `round` is a true semicircle of radius `stroke/2`; assert `miter` falls back to `bevel` past the limit and is never trusted on near-collinear boundaries; assert each degenerate case of §8.3 produces the named error rather than a malformed contour.
   **Differential test against a known-good stroker** — render the same path+width+caps+joins through any conforming SVG stroker and compare. With SVG semantics *and* constant width, the output should match a conforming stroker essentially exactly, so this is the strongest oracle in the whole suite and should be the primary test rather than a supplement.
4. **Curvature validation** — the §9.2 decision means there are no booleans to test. Instead: fuzz random paths against random widths and assert that the analytic `stroke/2` vs. curvature-radius check fires exactly when the generated offset folds back within a segment interior. Verifying the check against ground truth is the whole test, since the check *replaces* resolution. Plus winding-direction normalization per target format (§9.3).

   **Fills** get their own three assertions, because their failure modes are different: a filled closed path emits its own outline as one contour; a fill nested inside a fill renders a hole; and `stroke` plus `fill` on one closed path renders solid, verified by rasterizing it against the stroke-only outer contour and comparing coverage rather than comparing point lists. Then fuzz self-intersecting outlines and assert the §9.2 detector fires on each — its false-negative rate is the thing that matters, since a missed crossing ships a glyph with a vanished lobe.
5. **Export** — `fonttools ttx` round-trip, `ots-sanitize` (OpenType Sanitizer), and FontBakery checks in CI; render a pangram at 8–48 ppem with FreeType and diff against golden rasters.

Plus one end-to-end acceptance test: build a small but real typeface (uppercase + lowercase + digits + basic punctuation) in three weights from one source, install it, and set text in it.

---

## 17. Downstream implementation plans

This document exists to make these five plans writable independently. Suggested order — 1 and 2 gate everything and are both small; **3 is by a wide margin the largest and the schedule risk**; 4 and 5 can proceed in parallel once 3 lands.

Plan 3 is where the effort and the correctness risk live. Directed construction makes the evaluation engine a topological sort, and the SVG stroke model removes every region boolean — but it also makes offset approximation the *only* path through the envelope stage, on every stroke, with no exact case to fall back on.

1. **DSL surface** — grammar, lexer, lossless syntax tree, AST lowering, scope resolution, stable node identity, formatter. Written against spec §5, which is normative and exhaustive; §3, §4, §6, and §10.3 here supply rationale only. Read first: per-field validation of string-valued enums (spec §5.5) and element-typed tuples (spec §5.8).
2. **Evaluation engine** — dependency graph construction from name references, topological evaluation, cycle detection with full-path reporting, the construction library (§6.3), per-glyph scoping, incremental re-evaluation. (§5, §6.3, §14) **This is now a small plan.** It was the largest before the directed-construction decision; sequence it early precisely because it is cheap and everything downstream needs it.
3. **Geometry kernel** — paths (lines, quadratic and cubic Béziers, elliptical arcs), constant-width offset generation, caps and joins inside the offset walk, the analytic curvature check, filled-contour emission with its self-intersection detector, contour roles and winding normalization. **No variable width, no region booleans, no half-plane clipping, no planar arrangement, and no self-intersection resolution** (§1.1, §7.3, §9.2). (§7, §8, §9) Sequence: offsets + caps + joins → curvature check → fills → roles → winding normalization.
4. **Font compiler** — extrema insertion, zone-aware quantization, cu2qu, table assembly for TTF/OTF/WOFF2, hint derivation, metrics, kerning, instances. (§11, §12, §13, §15)
5. **Editor projection layer** — structured edits per tool, inverse drag, dependency inspection, partial-text tolerance, incremental redraw. (§10)

---

## 18. Residual risks

One design question is deliberately open — the reuse mechanism (A.2), deferred until a real face shows which duplication actually matters. Everything else is decided in §1.

The risk profile has shifted decisively toward *scope* and *ergonomics* and away from *correctness*. Constant width per path, SVG stroke semantics, and no region operators between them removed the variable-width envelope, the planar arrangement, and the support-function apparatus — which were the three hardest correctness problems. What remains, ranked:

1. **The parametric ceiling, not the expressive one (§7.3)** — `fill` means almost any shape is *expressible*; the risk is what it costs to express it. A design that leans on fills is a design the system no longer reasons about: it sweeps no parameters on its own, yields no stem hints (§12.3), and needs its self-intersection watched (§9.2). Combined with no abstraction mechanism (A.2), the failure mode is not "cannot build this face" but "built it, and it is a hand-maintained outline font wearing a parametric source." Mitigation: build a real face early and **measure what fraction of its ink comes from fills** — the §16 acceptance test is how that gets known rather than guessed.
2. **Inverse drag (§10.2)** — the least precedented piece, and load-bearing. With pure directed construction there are no free variables and no solver to absorb a drag, so every drag on derived geometry must resolve to "rewrite one upstream literal." If that feels wrong, the editor is a compiler with a preview window regardless of how good the engine is. Mitigation: prototype it against a hand-written source file before the editor exists.
3. **The §9.2 curvature-error decision** — erroring on tight curvature rather than resolving self-intersection keeps intersection code out of the stroke path, which makes this a usability rather than correctness risk: `stroke` is swept across instances, so a design that builds at Regular can fail at Bold. Mitigation: make the error name the exact parameter value at which it begins, surface it in the parameter-sweep preview (§10.1) before export, and keep resolution as a known fallback. The filled-contour detector is the same decision applied to a case with no analytic shortcut — detect and error, never resolve.
4. **The deferred reuse mechanism (A.2)** — the first real face gets written almost entirely by duplication, and retrofitting a mechanism afterwards means rewriting that source. Mitigation: A.2 records the candidates and the three constraints any of them must satisfy.
5. **Cycle diagnostics (§5.3, §14)** — a directed system's whole advantage over a solver is legible errors, and circular definition is the *only* structural failure mode. Because circular thinking is illegal in a place where designers may naturally reach for it, the message must name the full cycle path and suggest which edge to break. Build it with the graph, not after.
6. **Offset approximation quality (§8.1)** — still the only approximation in the geometry pipeline, but now the *sole* case, with conforming SVG strokers available as near-exact oracles (§16.3). Mitigation: tolerance an order of magnitude below quantization plus the differential test.
7. **TrueType hinting scope creep (§12.3)** — the recommendation to emit only metric-zone `prep` + `gasp` is deliberate. Treat any request for full TT instruction generation as a separate project with its own plan.

---

## Verification of this document

It is research, so verification is review rather than execution:

- Every locked decision in §1 traces to a section that specifies its mechanism.
- Every algorithm named in §8, §9, §11 has a published reference (the standard `4/3·tan(φ/4)` cubic arc approximation; Sederberg–Nishita Bézier clipping; cu2qu; standard tolerance-checked cubic offset fitting, §8.1).
- The spec's Appendix A sample exercises the grammar on sixteen real glyphs; the omissions it exposed are closed (A.1), and the one deferred decision is recorded with its constraints (A.2).
- Every decision here has a normative counterpart in the spec. A plan author who has to invent semantics has found a gap in the spec, not here.

---

## Appendix A — The sample, and what it exposed

The sixteen-glyph sample (`A`–`F`, `0`–`9`) lives in spec Appendix A and is the only copy. It was written to exercise the grammar on real letterforms; this appendix records what writing it taught.

**Why it reads monolinear.** Width is one constant per path (§7.3), so contrast lives *between* paths: stems and bowls at `stem`, bars and thin diagonals at `hair`. That is the design space the language supports well, so the sample demonstrates it rather than fighting it.

**The centreline convention trips everyone once.** `stroke` is symmetric about the path, so a bar whose top edge must touch cap height is centred at `capHeight.y - hair/2`, and a round top at `.ink - stem/2`. Earlier drafts of the sample centred bowls *on* `.ink` and so overshot by half a stem; the convention is easy to state and easy to break.

### A.1 What the sample exposed

Three genuine omissions, now closed in the spec:

1. **Transform constructors** — `nine` needs a composed transform. §6.3 now has `identity`, `translate`, `rotate`, `scale`, `slant`, `reflect`, and composition as a parenthesised sequence applied in reading order.
2. **`component` config** — fixed at `glyph:` plus exactly one of `offset:` or `transform:` (§11.1).
3. **Cross-glyph reads** — `glyphs.<name>` exposes only `advance`, `bbox`, and declared `anchor`s (§6.5). A glyph's `let`s and paths stay private, which keeps the cross-glyph graph narrow for §5.6's caching.

Two further changes the sample drove:

- **`stroke` collapsed to one constant per path.** The sample originally carried a path-level map keyed by segment name, which forced every modulated path to name its segments and produced keys like `right` and `left` that read as directions while sitting next to the constant `right` meaning the direction. Width is now a single `num` on the path (§7.3). That deleted the map, the `profile` type, the numeric-versus-named key rule, the naming pressure, *and* the whole variable-width envelope in §8 — at the cost of contrast within a stroke. `joinAt` is the only field still needing segment names.
- **Reserved words may not name declarations** (§6.5). Field-typed resolution made `cube right (…)` next to `c1: p + right * k` machine-unambiguous and human-unreadable. Cheap to enforce, and it kills the category.

The sample is what made that first trade legible: written out, none of these sixteen glyphs actually wanted within-stroke modulation, because at low contrast the difference lives between the stems and the bars. A high-contrast face would have made the same sample fail.

Two more the sample forced into the open:

- **Top-level `let` had nowhere to live.** The sample originally wrapped params, metrics, and lets in a block, which left "where does a file-scope `let` go" unanswered. Resolved in §6.1: there is **no wrapper block at all** — `font`, `param`, `metric`, and `let` are top-level directives, and everything declared there is referenced **bare**, with shadowing forbidden so each bare name has exactly one binding. Two things fall out: the `font.` prefix disappears from every expression (a real readability win at ~100 occurrences in this sample alone), and files compose by concatenation, so a face can span several files with no include mechanism. The cost is that a glyph can no longer name a path after a param — hence `path upright` rather than `path stem`.
- **The spacing model was unspecified, and the sample had it wrong.** `advance: w + 2*sidebear` measures bearings from *centrelines*, so every glyph was under-spaced by a stem width and had ink hanging left of its origin. §13.1 now mandates the ink model — first as `advance: glyph.bbox.width + 2*sidebear`, later refined to `glyph.bbox.x1 + sidebear` — with paths authored in final position. This is the clearest case in the whole exercise of a sample catching a real error rather than a missing feature. A monospace sample later showed that authoring in final position cannot centre ink, and `advance` gained optional `lsb` / `rsb` companions with an automatic horizontal shift.

One observation that needed recording rather than fixing:

- **Counters from overlapping open strokes need no boolean.** `B`'s bowls and `six`'s bowl-plus-spine enclose their counters between two *separate* contours; neither encloses the counter, so nonzero winding leaves it uncovered. Now stated in §7.4, because it looks like it should require a union and does not.

### A.2 Deferred: no reuse mechanism

The sample's most visible property is repetition. `E` and `F` differ by one bar and share three. `B` and `D` are both stem-plus-bowl. `zero` and both bowls of `eight` are the same four-segment loop at different sizes. Every glyph repeats the same `advance` expression verbatim, and every stem-led capital repeats `let sx = stem/2`. At 200 glyphs the source would be mostly duplication.

**Decision: ship no abstraction mechanism.** Revisit once a real face exists and the duplication patterns are known rather than guessed. The cost is a verbose source and a probable retrofit; the benefit is that the mechanism gets designed against evidence.

The analysis is recorded here so the decision is cheap to make later, and because **three of the four candidates conflict with decisions already made** — that constraint is durable and should not be re-derived.

| Candidate | Conflict |
|---|---|
| **`shape`** — parameterised bundles of paths | None. Needs four rules: inputs-plus-globals scope (anything looser is dynamic scoping and breaks §5.2's static graph); qualified inner names (`barT.bar`) so `stroke` keys and the editor have addresses; `out` values for shapes that must report a computed point; and **inverse drag confined to the call site**, or one drag silently changes every use. |
| **Glyph inheritance** — `glyph F from E` | `override` means a value's definition is in two places, which is what §5.1 exists to prevent. Worse, §10.2's hardest drag case becomes routine: clicking inherited geometry in `F` selects text in `E`, so every drag forks between "edit `E`, changing both" and "synthesize an override." Also no help for intra-glyph repetition. |
| **Functions returning `path`** | Forces `draw:` back into the language. §7.2's rule is *a path with `stroke` renders*; a path bound by `let` either renders (making `let` side-effecting) or needs a render statement — undoing the `path`/`stroke`/`draw` collapse. Also makes paths anonymous values with no stable name for `stroke` keys. |
| **Text macros** | Cheapest to build and the most powerful — can emit whole `glyph` blocks. But §10.3 makes the lossless CST the editor's document model, and with macros the canvas shows *expanded* geometry while the text holds an unexpanded call, so **no tree node corresponds to the rendered path**. Structured edits have nothing to target. Diagnostics also need source maps through the expander, which undercuts the quality claim that justified directed construction in the first place (§5.3). |

**The constraint for whoever revisits this:** the mechanism must not reintroduce `draw:` (§7.2), must not break the tree-node-per-rendered-thing property (§10.3), and must not put a definition in two places (§5.1). `shape` is the only one of the four that satisfies all three, which is where to start unless the evidence says otherwise.

Separately and independently: **bulk glyph generation** — accented forms, small caps, figure styles — is a loop or comprehension over a list, not a reuse construct. It can be added without touching any of the above.
