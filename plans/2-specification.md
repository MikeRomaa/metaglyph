# Metaglyph — Specification

## 1. Context

A typeface is defined as a program: named parameters, construction geometry, and algebraic relationships between them. Changing `stem` from 100 to 180 rebuilds every glyph. The text source is the design; the canvas is a projection of it.

This document specifies the language, the geometry model, the compilation pipeline, and the editor's structured-edit model. It is the reference the five implementation plans in §16 are written against. It does not choose programming languages or libraries. Rationale and rejected alternatives live in `1-research.md`; where the two disagree, this document governs.

---

## 2. Model

| Axis | Definition |
|---|---|
| Geometry resolution | Pure directed construction. One definition per value; a dependency graph; topological evaluation. No equations, no free variables, no solver. |
| Language shape | Two forms: `let name = expr;` and `kind name? (config)? { declarations }?`. Infix is used for arithmetic only. |
| Shape model | SVG semantics: a path may be stroked (`stroke`, a scalar width), filled (`fill`), or both. No pens. |
| Stroke width | One constant `num` per path, named `stroke`. Contrast between strokes exists between paths, not within one. |
| Caps and joins | SVG sets as validated string literals. Caps `"butt"` (default), `"round"`, `"square"`. Joins `"miter"` (default), `"round"`, `"bevel"`. |
| Region operators | None. No `trim`, `union`, `difference`, or `intersection`, and no filled-area value in the language. Ink is produced by stroking and filling; a path's or glyph's extent is readable via `.bbox` only. |
| Overlaps | Always kept. Overlapping contours of consistent winding are what ships. |
| Output | Static TTF / OTF / WOFF2, with kerning, CFF hinting, and a TrueType `gasp` table. No OpenType substitutions, no variable fonts. |
| Script scope | Arbitrary Unicode codepoints, variation sequences (`cmap` format 14), and component reuse of whole glyphs or single paths. No contextual reordering, cursive attachment, or mark attachment. |
| Abstraction | None. No user-defined functions, macros, shapes, or glyph inheritance. Every glyph is written in full. |
| Text ↔ editor | The DSL is canonical. Every editor action is a structured transform on the syntax tree. |
| Italics | `slant` is a per-instance shear. A true italic is a glyph set selected by instance (§5.6). |

### 2.1 Shaping mechanisms

Four mechanisms produce letterform detail. There is no cutting or subtraction.

| Mechanism | Handles |
|---|---|
| Caps and joins (§6.4) | Butt / round / square ends, asymmetric ends, corner treatments |
| The end tangents — set by the first and last segments' control points or arc geometry (§6.4) | Angled and sheared terminals, flat cuts on diagonals |
| Overlapping paths (§6.5) | Slab serifs, flat apexes and tops across several strokes, compound terminals |
| `fill` on a closed path (§6.5) | Arbitrary filled geometry, including contrast within one shape |

A `"butt"` cap is perpendicular to the path's tangent at that end. A cut at another angle is made by ending the skeleton with a short straight segment whose perpendicular is the intended cut, or by authoring the shape as a `fill`.

A filled outline responds to parameters only through the expressions written in its coordinates, and it yields no stem hints (§11.3).

---

## 3. Pipeline

```
source files
  → lex / parse                    → lossless syntax tree (CST)
  → lower, resolve, type-check     → AST + scope tree (§5)
  → build dependency graph         → per-glyph node sets (§4)
  → topological evaluation         → all values known (§4)
  → geometry realization           → skeleton paths (cubic Béziers) + stroke widths (§6)
  → offset generation              → per-stroke outline contours (§7);
                                     filled paths bypass this stage
  → fold trim                      → tight curves resolved (§7.2)
  → contour roles + winding        → roles and direction (§8)
  → slant                          → instance shear applied to outlines (§12.3)
  → extrema insertion              → on-curve points at x/y extrema (§10.2)
  → curve conversion               → quadratic (glyf) or cubic (CFF2) (§10.3)
  → zone snap + quantize           → integer design-unit outlines (§10.4)
  → hint derivation                → CFF blue zones and stem hints; TrueType gasp (§11)
  → metrics / kern build           → hmtx, GPOS (§12)
  → table assembly                 → TTF / OTF
  → repackage                      → WOFF2
```

Each stage is a pure function of the previous one. Re-evaluation after an edit re-runs a suffix of the pipeline, keyed off the dependency graph.

---

## 4. Evaluation

### 4.1 One definition per value

Every name is bound exactly once, by an expression over other bound names. `=` is definitional.

```
let hairline = stem * contrast;
let barY     = capHeight.y * 0.52;
let apex     = (w/2, capHeight.ink);
```

There is no assignment, no equation solving, no free variables, and no way to state a partial fact about a value. Mutual and circular definitions are illegal.

### 4.2 Algorithm

1. Resolve names. Every expression's free identifiers become edges in the dependency graph.
2. Topologically sort the graph, breaking ties by declaration order (§14).
3. Evaluate in that order. Each node is evaluated once.

Every operand is known when its operation runs, so every function in §5.9 has no preconditions beyond its own domain.

Declaration order is irrelevant to results: the graph is built from references, not statement order. A glyph's config may reference its body and vice versa.

### 4.3 Cycles

Graph construction fails in two ways: an unresolved name and a cycle. Report a cycle as the full path, with file and line for each hop, and a suggested edge to break:

```
error: circular definition
  stemAxis  → depends on → leftEdge   (glyph n, line 14)
  leftEdge  → depends on → stemAxis   (glyph n, line 11)
  hint: derive one from a parameter instead
```

### 4.4 Scoping

Two value scopes exist: the file's top level, and each glyph (its config and body together). A glyph sees top-level declarations and its own declarations, all bare. Shadowing is an error, so every bare name has exactly one binding. The full rules, including non-value namespaces, are in §5.11.

`glyph.*` is readable only inside a glyph. Top-level `let`s and `kern` fields may read `glyphs.<name>.*`.

### 4.5 Incremental re-evaluation

An edit dirties the node it touches and everything downstream. Re-evaluate that subgraph only.

### 4.6 Failure containment

An error at a node marks that node and every node downstream of it as failed. Evaluation continues for every node not downstream of a failure, so one run reports every independent error. A failed top-level node is reported once, at its own source location, not once per dependent glyph.

A build with any error writes no output file. Warnings (§13) do not block output.

---

## 5. Language reference

This section is normative and exhaustive. Nothing in the language is implied by example alone.

Notation: `?` marks an optional field, `|` alternatives, `T*` a list of `T`. "Required" means omission is a compile error.

### 5.1 Lexical structure

| Token class | Definition |
|---|---|
| Comment | `//` to end of line. No block comments. |
| Identifier | `[A-Za-z_][A-Za-z0-9_]*`, case-sensitive |
| Number | `[0-9]+ ( "." [0-9]+ )?` — unsigned; a leading `-` is unary minus |
| Suffixed number | A decimal number immediately followed (no whitespace) by `deg`, `rad`, `em`, or `%` |
| Hex integer | `0x` followed by one or more hex digits, either case; value at most 2^53 |
| Codepoint integer | `U+` followed by 4–6 hex digits, either case; value at most `U+10FFFF` |
| Character integer | `'` exactly one Unicode scalar value `'`, any scalar not only ASCII; escapes `\'` `\\` `\n` `\t`. Its value is the scalar's codepoint. |
| String | `"` … `"`; escapes `\"` `\\` `\n` `\t`. No interpolation, no multiline. |
| Boolean | `true` `false` |
| Punctuation | `(` `)` `{` `}` `[` `]` `,` `;` `:` `.` `..` |
| Operators | `+` `-` `*` `/` `^` `<` `<=` `>` `>=` `==` `!=` `and` `or` `not` |

Hex, codepoint, and character integers are plain `num` values that are exact integers. They are not a distinct type and differ from a decimal number only in spelling: `65`, `0x41`, `U+0041`, and `'A'` are the same value. None of them takes a suffix. Each is validated where it is written, as a syntax error: a hex integer above 2^53, a codepoint above `U+10FFFF`, and a character literal that is empty, holds more than one scalar value (a decomposed `é` is two), or uses an unknown escape.

Whitespace and newlines are insignificant except as comment terminators and inside strings and character literals. There is no line-continuation rule and no significant indentation.

`{ … }` is a body after a block header and a map literal in expression position. The two never occur in the same position.

### 5.2 Grammar

```
let <name> = <expression>;
<kind> <name>? ( <config> )? { <declarations> }?

range  := bound ".." bound
bound  := "-"? ( number | suffixed-number )
```

Parens hold configuration; braces hold declarations. Both parts are optional.

| Rule | |
|---|---|
| `let` statements | terminated by `;` |
| Config fields | `name: value`, separated by `,`, trailing comma allowed |
| Declarations in a body | no separator |
| Body ordering | significant only for path segments |

A config field's value is an expression, except for fields typed `identifier`, `range`, `glyphref`, `groupref`, or `map`, whose syntax is given with the field.

A **constant expression** contains only literals, suffixed literals, arithmetic operators, and `math.*`. It references no declaration.

`( a )` is grouping. `( a, b, … )` with two or more elements is a **tuple**, typed by its elements (§5.8).

Structure is never infix: there are no path operators, clause keywords, or trailing modifiers. Infix applies to arithmetic on numbers, pairs, and booleans.

### 5.3 Units and coordinate space

- One internal unit is one design unit. All reals are `f64`.
- Angles are radians internally. An unsuffixed number in an angle position is radians.
- Suffixes: `deg` multiplies by π/180; `rad` by 1; `em` by `font.em`; `%` by 0.01. A suffixed number is a `num` and is legal anywhere a `num` is.
- An `int`-typed field takes a `num` expression whose value must be an exact integer; any other value is an error.
- Y is up. Angles are counter-clockwise from +x.
- The glyph origin is `x = 0`. Paths are authored in final position (§12.1).

### 5.4 Reserved words

May not be used as a declaration name. The list is closed.

- **Declaration keywords:** `font` `param` `metric` `let` `glyph` `instance` `group` `kern` `path` `anchor` `component` `start` `line` `quad` `cube` `arc` `close`
- **Built-in constants:** `up` `down` `left` `right` `identity`
- **Literals:** `true` `false`
- **Operator words:** `and` `or` `not`
- **Namespace roots:** `font` `glyph` `glyphs` `instance` `math`

Enum-valued fields take string literals, so `butt` `round` `square` `miter` `bevel` `top` `bottom` `ccw` `cw` are not reserved and may be used as declaration names. Function names (§5.9) are not reserved; call position and value position resolve separately (§5.11).

### 5.5 Types

Types are checked statically after name resolution. Every type error is reported before evaluation.

| Type | Members / notes |
|---|---|
| `num` | f64 |
| `int` | A `num` whose value is an exact integer (§5.3) |
| `bool` | |
| `string` | |
| `pair` | `.x` `.y`; constructed `(a, b)` |
| `point` | Alias of `pair` |
| `line` | Infinite line with an origin and a direction (§5.9). Opaque; from §5.9 constructors only |
| `ellipse` | Axis-aligned ellipse. `.center` `.rx` `.ry`; from `ellipse` / `circle` (§5.9) only |
| `transform` | Affine. Opaque; from §5.9 constructors, `identity`, or a transform sequence (§5.8) |
| `path` | `.bbox`; otherwise opaque. A declared path, referenced by name, or the result of `subpath` / `reverse` |
| `rect` | `.x0` `.y0` `.x1` `.y1` `.width` `.height` `.center` |
| `zone` | `.y` `.ink` `.overshoot`; from a `metric` declaration |
| `range` | `a..b` (§5.2); only as `param` `range:` |
| `list<T>` | `[ a, b, c ]`, homogeneous |
| `map<K,V>` | `{ k: v, … }`; used by `joinAt` |

No value is ever partially determined.

`cap`, `join`, `align`, and `sweep` are not distinct types. They are `string`, with the legal set enforced by the field's validator:

| Field | Legal values |
|---|---|
| `caps`, and each element of a `caps` tuple | `"butt"` `"round"` `"square"` |
| `joins`, `joinAt.*` | `"miter"` `"round"` `"bevel"` |
| `align` | `"top"` `"bottom"` |
| `arc` `sweep` | `"ccw"` `"cw"` |

Case is significant. Validation is per field, so a diagnostic enumerates the legal set: `joins: "mitre"` → *unknown join "mitre"; expected one of: miter, round, bevel*.

### 5.6 Top-level directives

Exactly these eight. A font is an ordered list of source files; all directives from all files are visible everywhere. The compiler receives the file list explicitly. Order matters only for tie-breaking (§14).

**`font ( … )`** — no body, exactly one per font.

| Field | Type | |
|---|---|---|
| `name` | `string` | required |
| `em` | `int` | required; an integer literal between 16 and 16384 |
| `version` | `string` | optional, default `"1.000"`; form `digits.digits`. Feeds `head.fontRevision` and name ID 5 |
| `designer` `foundry` `license` | `string` | optional |

**`param <name> ( … )`** — no body.

| Field | Type | |
|---|---|---|
| `default` | `num` | required; constant expression |
| `range` | `range` | optional; `default` and every instance override must lie within it, inclusive |

A param may not be named `slant`, `glyphset`, `styleName`, `weightClass`, or `widthClass`; those are instance fields.

**`metric <name> ( … )`** — no body.

| Field | Type | |
|---|---|---|
| `y` | `num` | required; may reference params and top-level `let`s |
| `overshoot` | `num` | optional, default `0`; must be ≥ 0 |
| `align` | `string` | optional, default `"top"` |

`.y` is the flat position; `.ink` is where a round glyph reaches: `y + overshoot` when `align` is `"top"`, `y − overshoot` when `"bottom"`.

`baseline`, `xHeight`, `capHeight`, `ascender`, `descender` are required declarations; omitting any is an error. `baseline.y` must evaluate to `0`. Additional metrics are unrestricted in name and count and contribute blue zones only.

**`let <name> = <expression>;`**

**`glyph <name> ( … ) { … }`**

| Field | Type | |
|---|---|---|
| `codepoint` | `int` \| `int*` | optional; constant expression; each value in `0`–`0x10FFFF`; illegal when `glyphset` is present |
| `variation` | `pair` \| `pair*` | optional; constant expression; each pair is `(base, selector)`, both integers; illegal when `glyphset` is present; see below |
| `advance` | `num` | optional; see below |
| `lsb` | `num` | optional; the left sidebearing; see below |
| `rsb` | `num` | optional; the right sidebearing; see below |
| `glyphset` | identifier | optional |

Body contains `let`, `path`, `anchor`, `component`.

A glyph declares one or two of `advance`, `lsb`, `rsb`. Declaring none, or all three, is an error. Together they fix the glyph's advance and a horizontal **shift** applied to its ink (§12.1).

**Variation sequences.** `variation` maps Unicode variation sequences to the glyph: a base character followed by a variation selector, the way `codepoint` maps single characters. Each `(base, selector)` pair has `base` in `0`–`0x10FFFF` and `selector` a variation selector: VS1–VS16 (`U+FE00`–`U+FE0F`) or VS17–VS256 (`U+E0100`–`U+E01EF`). A glyph may declare `variation` with or without `codepoint`; a glyph with only `variation` is reached through its sequences alone.

```
glyph zero (codepoint: '0', rsb: sidebear) { … }
glyph zero_vs1 (variation: ('0', U+FE00), advance: glyphs.zero.advance) { … }  // DIGIT ZERO, short diagonal stroke form
```

`variation: [('0', U+FE00), (U+2229, U+FE00)]` declares several. The output is `cmap` format 14 (§10.6).

A glyph without `glyphset` belongs to the **default set**. A glyph with `glyphset: S` is the alternate definition, in set `S`, of the default-set glyph with the same name. That default-set glyph must exist, and the alternate takes its codepoints and variation sequences. A name has at most one definition per set. A glyph set exists if and only if at least one glyph names it.

**`instance <name> ( … )`** — no body.

| Field | Type | |
|---|---|---|
| `<paramName>` | `num` | any declared `param`; constant expression; overrides its default |
| `slant` | `num` | optional, default `0`; an angle; constant expression |
| `glyphset` | identifier | optional; must name an existing glyph set |
| `styleName` | `string` | optional, default the instance's name |
| `weightClass` | `int` | optional, default `400`; 1–1000 |
| `widthClass` | `int` | optional, default `5`; 1–9 |

When the source declares no instance, the build uses one implicit instance, `instance Regular ()`.

**`group <name> ( glyphs: <glyphref>* )`** — no body. The list is non-empty and names default-set glyphs.

**`kern ( left: <glyphref>|<groupref>, right: <glyphref>|<groupref>, by: num )`** — no body, all fields required. Kerning rules are in §12.2.

### 5.7 Nested declarations

**`path <name>? ( … ) { … }`** — in a glyph body.

| Field | Type | |
|---|---|---|
| `stroke` | `num` | optional. Constant along the path; must be > 0 |
| `fill` | `bool` | optional, default `false`. Requires the path to be closed. Combines freely with `stroke` |
| `caps` | `string` or `(string, string)` | optional, default `"butt"`. A string sets both ends; a 2-tuple sets `(start, end)`. Requires `stroke`. A closed path has no ends, so on one `caps` is ignored, with a warning |
| `joins` | `string` | optional, default `"miter"`. Requires `stroke` |
| `joinAt` | `map<segmentName, string>` | optional. Requires `stroke` |

Body contains `start`, `line`, `quad`, `cube`, `arc`, `close`. Order is significant. A path must have a body.

The segment declarations follow SVG path data: `start` is moveto, `line` lineto, `quad` and `cube` the quadratic and cubic curveto, `arc` an elliptical arc, and `close` closepath. Each segment runs from the **current point** — the previous declaration's endpoint — to its own `to`. Its geometry is fixed by its own fields and the current point, plus, for an omitted control point, the previous segment (§6.3).

**`start <name>? ( at: point )`** — exactly one per path body, first. `at` required; it sets the current point.

**`line <name>? ( to: point )`** — `to` required. No other fields.

**`quad <name>? ( … )`** — a quadratic Bézier.

| Field | Type | |
|---|---|---|
| `to` | `point` | required |
| `c` | `point` | optional — the control point. Omissible only when the previous declaration is a `quad`; it is then the reflection of that quad's control point (§6.3) |

**`cube <name>? ( … )`** — a cubic Bézier.

| Field | Type | |
|---|---|---|
| `to` | `point` | required |
| `c1` | `point` | optional — the first control point. Omissible only when the previous declaration is a `cube`; it is then the reflection of that cube's `c2` (§6.3) |
| `c2` | `point` | required — the second control point |

**`arc <name>? ( … )`** — an arc of an axis-aligned ellipse (§6.3), in one of two modes. **Centre mode** declares `center`, and the radii are solved. **Radii mode** declares `rx` and `ry`, and the centre is solved. `center` is mutually exclusive with `rx` and `ry`; `rx` and `ry` require each other; one mode is required.

| Field | Type | |
|---|---|---|
| `to` | `point` | required |
| `sweep` | `string` | required — `"ccw"` or `"cw"`, the direction of travel from the current point to `to` about the centre (y-up) |
| `center` | `point` | centre mode — the ellipse's centre |
| `rx` | `num` | radii mode — the horizontal radius; must be > 0 |
| `ry` | `num` | radii mode — the vertical radius; must be > 0 |
| `large` | `bool` | radii mode only; optional, default `false` — selects the arc spanning more than 180° |

**`close`** — at most one per path body, last. No name, no config, no body. It declares the path closed and appends the closing segment, a straight line from the last endpoint back to the `start` point.
- When the last declaration already ends at the `start` point, the closing segment is omitted and the path is still closed. A closed curve is therefore written with a final curve segment ending at the `start` point, followed by `close`.

A path is closed if and only if its body declares `close`.

**`anchor <name> ( at: point )`** — in a glyph body. The name is a glyph-scope value (§5.11) and may not be `advance` or `bbox`.

**`component ( glyph: glyphref | path: path, offset: pair?, transform: transform?, stroke: num?, fill: bool?, caps: …?, joins: …?, joinAt: …? )`** — exactly one of `glyph` and `path`. `offset` and `transform` are mutually exclusive; `offset: (dx, dy)` means `transform: translate(dx, dy)`. Call the placement `M`.

- **Glyph component** — `glyph` names a default-set glyph; in an instance that selects a glyph set, the reference resolves to that glyph's alternate when one exists. It draws that glyph's whole outline (§10.1). `stroke`, `fill`, `caps`, `joins`, and `joinAt` are illegal here.
- **Path component** — `path` is any expression of type `path`: a path name in this glyph (`path: stem`), another glyph's path (`path: glyphs.o.bowl`, §5.10), or a `subpath` / `reverse` result. It draws that path's skeleton transformed by `M`, as a rendering path of this glyph in its own right (§6.2):
  - It takes the referenced path's `stroke`, `fill`, `caps`, `joins`, and `joinAt`. Each one the component declares replaces the referenced path's own, with the same types and rules as on a `path` (§5.7 path fields).
  - The skeleton is transformed, then stroked: `stroke` is the width on the page, whatever `M` scales by, and round caps and joins stay round. `fill` requires the referenced path to be closed.
  - A component that ends up with neither `stroke` nor `fill` draws nothing and is an error. A construction path (§6.1) or a `subpath` result renders only through a component that gives one.
  - `joinAt` keys name the referenced path's segments.

### 5.8 Operators and precedence

Highest binding first; all binary operators are left-associative except `^`.

| Level | Operators |
|---|---|
| 1 | `.` member access, `(` call, `( )` grouping |
| 2 | `^` (right-associative; its right operand may carry a unary operator, as in `2^-1`) |
| 3 | unary `-`, `not` |
| 4 | `*` `/` |
| 5 | `+` `-` |
| 6 | `<` `<=` `>` `>=` |
| 7 | `==` `!=` |
| 8 | `and` |
| 9 | `or` |

`-2^2` is `-(2^2)` = `-4`.

| Operands | Operators |
|---|---|
| `num`, `num` | all arithmetic and comparison operators |
| `pair`, `pair` | `+` `-` componentwise; `==` `!=` |
| `pair`, `num` | `pair * num`, `num * pair`, `pair / num` |
| `bool`, `bool` | `and` `or` `==` `!=`; unary `not` |
| `string`, `string` | `==` `!=` |

`pair * pair` is not defined — use `dot` or `cross`. Division by zero is a domain error. `0^0` is `1`; a negative base with a non-integer exponent is a domain error.

**Tuples** are typed by their elements:

| Elements | Type |
|---|---|
| exactly two, both `num` | `pair` |
| two or more, all `transform` | transform sequence (a `transform`) |
| exactly two, both `string` | string pair; legal only as `caps` (§5.7) |
| anything else | type error |

A transform sequence composes its elements in reading order: `(rotate(180deg), translate(w, h))` rotates first, then translates. A single transform needs no parentheses.

### 5.9 Functions

Complete. Nothing outside this list is callable. Angle arguments and results are radians.

**Scalar → `num`:** `abs` `sign` `floor` `ceil` `round` (half away from zero) `sqrt` `exp` `log` (natural) `sin` `cos` `tan` `asin` `acos` (one `num` each) · `atan2(y, x)` `min(num,num)` `max(num,num)` · `clamp(x, lo, hi)` `lerp(a, b, t)`

**Pair:** `length(pair)→num` · `angle(pair)→num` · `unit(pair)→pair` · `dir(θ)→pair` · `dot(pair,pair)→num` · `cross(pair,pair)→num` (z-component) · `perpendicular(pair)→pair` (rotated +90°)

**Point construction:** `meet(line,line)→point` · `mediate(a, b, t)→point` (`a + (b − a)·t`) · `project(point,line)→point` (perpendicular foot) · `polar(p, len, θ)→point` · `mirror(point,line)→point`

**Line construction:** `lineThrough(point,point)→line` · `lineAt(p, θ)→line` · `hline(y)→line` · `vline(x)→line`. `line` is a declaration keyword; the line constructor is `lineThrough`.

Every line has an **origin** and a unit **direction**, fixed by its constructor: `lineThrough(a, b)` has origin `a` and direction `unit(b − a)` (`a == b` is a domain error); `lineAt(p, θ)` has origin `p` and direction `dir(θ)`; `hline(y)` has origin `(0, y)` and direction `right`; `vline(x)` has origin `(x, 0)` and direction `up`. Origin and direction only matter to the ray queries below; `meet`, `project`, `mirror`, and `reflect` treat the line as infinite and unoriented.

**Ellipse construction:** `ellipse(center, rx, ry)→ellipse` · `circle(center, r)→ellipse` (`rx = ry = r`). The ellipse is axis-aligned, like an `arc`'s (§6.3), so an `arc (center: e.center, …)` or `arc (rx: e.rx, ry: e.ry, …)` between two points on `e` lies on `e`. A radius ≤ 0 is a domain error. A rotated ellipse is not expressible.

**Line–ellipse queries.** Distances are measured from the line's origin along its direction; negative distances lie behind the origin.

`crossings(line, ellipse)→num*` (distances where the infinite line crosses the ellipse, ascending: two values, or one when the line is tangent within `ARC_TOLERANCE` (§14), or empty) · `along(line, s)→point` (`origin + s · direction`) · `cast(line, ellipse)→point` (treats the line as a ray: the point of the smallest crossing distance greater than `ARC_TOLERANCE`; a domain error when there is none)

`cast(lineAt(e.center, θ), e)` is the point where the ray from the centre at angle `θ` meets `e`. A ray from a point on the ellipse skips that point and finds the far side. For the crossing behind the origin, or the farther of two, use `along(l, minOf(crossings(l, e)))` or `maxOf`.

**Transform construction:** `translate(dx, dy)` · `rotate(θ)` (about the origin, counter-clockwise) · `scale(s)` · `scale(sx, sy)` · `slant(θ)` (`(x, y) → (x + y·tan θ, y)`) · `reflect(line)` · `apply(transform, point)→point`

**Path queries.** A path with `n` segments (counting a non-omitted closing segment) has parameter domain `[0, n]`; segment `i` (0-based) spans `[i, i+1]` with its own Bézier parameter. An `arc` realized as `m` cubic pieces (§6.3) divides its span uniformly: piece `k` spans `[i + k/m, i + (k+1)/m]`. A parameter outside the domain is a domain error. Queries read the skeleton, never the stroked outline.

`pointAt(path, t)→point` · `directionAt(path, t)→pair` (unit tangent) · `curvatureAt(path, t)→num` (signed, positive when turning counter-clockwise) · `arcLength(path)→num` · `pointAtLength(path, s)→point` (`s ∈ [0, arcLength]`) · `intersect(a, b)→num*` (parameters on `a` where `a` crosses `b`, ascending; empty when none) · `subpath(path, t0, t1)→path` · `reverse(path)→path` · `extrema(path)→num*` (parameters where `x′ = 0` or `y′ = 0`, ascending)

A path returned by `subpath` or `reverse` is a construction value: it can be passed to path queries and read with `.bbox`. It renders only through a path component (§5.7) that gives it a `stroke` or `fill`.

**Reductions:** `sum(num*)→num` · `minOf(num*)→num` · `maxOf(num*)→num`. `minOf` and `maxOf` of an empty list are domain errors; `sum` of an empty list is `0`.

Extent is not a function. `.bbox` is a member on a `path`, on `glyph`, and on `glyphs.<name>` (§5.10).

### 5.10 Constants and namespaces

**Built-in constants:** `right` = `(1,0)` · `up` = `(0,1)` · `left` = `(-1,0)` · `down` = `(0,-1)` · `identity` (the identity `transform`).

**`math.*`:** `math.pi` `math.tau` `math.e`

**`font.*`** — the `font` directive's attributes only, never params or metrics: `font.name` `font.em` `font.version` `font.designer` `font.foundry` `font.license`. An unset optional attribute reads as `""`.

**`glyph.*`** — the current glyph, read-only:

| Member | Type | |
|---|---|---|
| `glyph.name` | `string` | |
| `glyph.codepoints` | `int*` | |
| `glyph.advance` | `num` | |
| `glyph.bbox` | `rect` | Tight bounds of all ink from this glyph's rendering paths and components, in authored coordinates. A domain error on a glyph with no ink |

Inside a glyph, everything — paths, `let`s, its own anchors, `glyph.bbox` — is in **authored coordinates**, before the §12.1 shift.

**`glyphs.<name>.*`** — another glyph: exactly `.advance`, `.bbox`, its declared anchors, and its named paths. It resolves to the glyph the current instance builds under that name (the glyph-set alternate when one is selected). A glyph's `let`s are not externally visible. `.bbox`, anchors, and paths read this way are in that glyph's **authored coordinates**: as written in its body, without its shift (§12.1). Its `lsb` or `rsb` places it in its own advance only; reading it from another glyph, or drawing it as a component, does not carry that placement along. A path read this way is a `path` value like any other: path queries take it, and a path component (§5.7) draws it.

**`instance.*`:** `instance.name` (`string`) · `instance.slant` (`num`)

### 5.11 Name resolution

Four namespaces exist.

| Namespace | Members | Referenced by |
|---|---|---|
| Value — top-level scope | `param`, `metric`, top-level `let` | bare identifiers |
| Value — glyph scope, one per glyph | the glyph's `let`s, path names, anchor names | bare identifiers inside that glyph |
| Glyph (font-wide) | glyph names and group names | `glyphref` / `groupref` fields, `glyphs.<name>` |
| Glyph set (font-wide) | glyph-set names | `glyphset:` fields |
| Segment (one per path) | the path's segment names | `joinAt` keys |

Rules:

1. A bare identifier followed by `(` resolves only against §5.9 functions.
2. Any other bare identifier in value position resolves, in order: the enclosing glyph scope → the top-level scope → §5.10 built-in constants. Anything else is an unresolved-name error.
3. Shadowing is an error: a glyph-scope name may not reuse a top-level name.
4. A duplicate name within one namespace is an error. Glyph and group names share one namespace. Glyph-set alternates share their default glyph's name by design (§5.6) and are not duplicates.
5. Reserved words (§5.4) may not be declaration names in any namespace.
6. Declaration order is irrelevant; only cycles are errors.

Enum values are string literals and never enter any namespace.

### 5.12 Absent

No user-defined functions, macros, shapes, or glyph inheritance · no loops, recursion, or comprehensions · no conditionals · no indexing · no assignment or mutation · no equations or free variables · no block comments · no string interpolation · no imports or includes · no infix path construction · no region operators, filled-area value, or area query · no variable width · no user-defined operators, caps, or joins · no scopes beyond those in §5.11 · no opaque-text block of any kind · no OpenType substitutions (§12.2).

---

## 6. Geometry

### 6.1 Construction entities

Construction geometry never renders.

1. **Points** — literal, or derived via the §5.9 constructors.
2. **Lines** — `lineThrough`, `lineAt`, `hline`, `vline`. Infinite, with an origin and a direction (§5.9).
3. **Ellipses** — `ellipse`, `circle`. Axis-aligned; exact, never approximated by curves.
4. **Metric guides** — `metric` declarations (§5.6), exposing `.y`, `.ink`, `.overshoot`.
5. **Construction paths** — a `path` with neither `stroke` nor `fill`.
6. **Measurements** — any `let`; the editor displays named scalars and pairs as dimensions.

### 6.2 `path` is the only shape construct

A path renders if and only if it declares `stroke`, `fill`, or both.

| Need | Mechanism |
|---|---|
| Skeleton that doesn't render | omit both `stroke` and `fill` |
| Stroked outline of a skeleton | `stroke: <num>` |
| Filled interior | `fill: true`, with `close` |
| Filled shape with a stroked border | both, on one path |
| The same stroke again, moved or mirrored | a path component (§5.7) |
| Render order | declaration order |

Render order does not affect appearance: overlaps are kept and nonzero winding is order-independent. It fixes contour order in the output only.

`p.bbox` is the tight bounds of the path's ink when the path renders, and of its skeleton otherwise.

### 6.3 Segments

A path body is a chain of segment declarations, each naming the point it arrives at. One declaration per point.

```
path bowl (stroke: stem) {           // t, r, b, l: the top, right, bottom, left of an ellipse about ctr
  start (at: t)
  arc   (center: ctr, to: r, sweep: "cw")
  arc   (center: ctr, to: b, sweep: "cw")
  arc   (center: ctr, to: l, sweep: "cw")
  arc   (center: ctr, to: t, sweep: "cw")
  close                              // already at t: no closing segment
}

path upright (stroke: stem) {
  start (at: (0, capHeight.y))
  line  (to: (0, 0))
  line  (to: (w, 0))
}
```

**Segment geometry.** `p` is the current point.

| Kind | Geometry |
|---|---|
| `line` | The straight segment `p` → `to`. |
| `quad` | The quadratic Bézier `p`, `c`, `to`. Realized as its exact degree-elevated cubic: `p`, `p + ⅔(c − p)`, `to + ⅔(c − to)`, `to`. |
| `cube` | The cubic Bézier `p`, `c1`, `c2`, `to`. |
| `arc` | The arc of an axis-aligned ellipse from `p` to `to`, travelling in the `sweep` direction; the ellipse is fixed by `center` or by `rx`, `ry`, and `large` (below). |

**Reflection.** An omitted control point is the reflection of the previous segment's adjacent control point through `p`, as in SVG's `S` and `T` commands:
- `cube` without `c1`: `c1 = 2·p − c2′`, where `c2′` is the previous `cube`'s `c2`.
- `quad` without `c`: `c = 2·p − c′`, where `c′` is the previous `quad`'s control point, whether written or itself reflected.

The previous declaration must be a segment of the same kind. The reflection makes the joint tangent-continuous. It is the only way one segment's geometry depends on another.

**Arcs.** An arc lies on an axis-aligned ellipse with centre `C` and radii `rx`, `ry`. Each mode declares half of that and solves the other half from the two endpoints, so neither mode is over-determined.

*Centre mode* (`center` declared, `C = center`). With `(dx₀, dy₀) = p − C`, `(dx₁, dy₁) = to − C`, `u = 1/rx²`, and `v = 1/ry²`, solve the linear system

```
dx₀²·u + dy₀²·v = 1
dx₁²·u + dy₁²·v = 1
```

- When it has a unique solution with `u > 0` and `v > 0`, that is the ellipse.
- When it is singular (determinant `dx₀²·dy₁² − dx₁²·dy₀²` of magnitude at most `1e-12 · |p − C|² · |to − C|²`), the endpoints are symmetric about an axis through `C` or about `C` itself, and they do not determine the ellipse. The arc is circular when `|p − C|` and `|to − C|` agree within `ARC_TOLERANCE` (§14), with their mean as its radius. Otherwise it is a geometry error, whose help names radii mode: a half-oval from the top of an oval to its bottom is `arc (to: b, rx: w/2, ry: h/2, sweep: "cw")`.
- Any other case is a geometry error: no axis-aligned ellipse about `center` passes through both endpoints.

*Radii mode* (`rx` and `ry` declared). This is SVG's endpoint arc (SVG 1.1 Implementation Notes F.6.5) with zero rotation. With midpoint `M = (p + to)/2`, half-chord `h = (p − to)/2`, and `h′ = (h.x/rx, h.y/ry)`, let `Λ = |h′|²`.

- When `Λ > 1`, the chord is longer than the ellipse can span. If `(√Λ − 1) · max(rx, ry) ≤ ARC_TOLERANCE`, take `Λ = 1`: the chord is a diameter. Otherwise it is a geometry error; unlike SVG, the radii are never enlarged.
- The two candidate centres are `C = M ± √((1 − Λ)/Λ) · (−rx·h′.y, ry·h′.x)`. Travelling in the `sweep` direction, one candidate's arc spans less than 180° and the other's more. `large: false` takes the first; `large: true` the second.
- When `Λ = 1` the candidates coincide at `M`, the arc spans exactly 180°, and `large` has no effect.

The arc runs from `p`'s eccentric angle to `to`'s, in the `sweep` direction, covering strictly between 0° and 360°. `to` equal to `p` is a zero-length segment (§7.3); a full ellipse takes at least two arcs. A rotated ellipse is not expressible: split it into `cube`s.

The arc is realized as `m = ⌈Δ / 90°⌉` cubic pieces of equal eccentric-angle span `φ = Δ/m`, where `Δ` is the swept angle. Each piece from angle `θa` to `θb` has its control points at `E(θa) + k·E′(θa)` and `E(θb) − k·E′(θb)`, where `E` is the ellipse's parametric form, `E′` its derivative, and `k = 4/3 · tan(φ/4)`. The first piece starts at `p` and the last ends at `to` exactly, even where an endpoint lies within `ARC_TOLERANCE` of the ellipse rather than on it (the circular fallback and the diameter case); every other point comes from `E`. This realization is exact per this definition; it is not a tolerance.

**Tangents.** A segment's tangent at an end is its derivative there. Where a control point coincides with that end, the tangent is the direction to the nearest distinct control or end point. A joint is smooth exactly when the incoming and outgoing tangents agree; nothing is inherited from one segment to the next except through reflection.

**Naming.** A segment's name denotes its endpoint and lives in the path's segment namespace (§5.11). `joinAt` is the only field that consumes segment names.

**Closing.** A closed path has no ends. Stroking it yields two contours (§8.1).

**Structural errors:**
- no `start`, or more than one `start`
- a field illegal on its declaration (§5.7)
- a `cube` without `c1`, or a `quad` without `c`, whose previous declaration is not a segment of the same kind
- a `joinAt` key naming no segment
- more than one `close`, or any declaration after `close`

### 6.4 Stroke width, caps, joins

A rendered stroke is the set of points within `r = stroke/2` of the skeleton, with the ends and corners shaped by caps and joins. It is equivalently a circular pen of diameter `stroke`. It is one constant per path.

Caps are defined at each end of an open path over the side-boundary endpoints `L = p(end) + r·n̂` and `R = p(end) − r·n̂`:

| Cap | Definition |
|---|---|
| `"butt"` | The straight chord `L`→`R`, perpendicular to the end tangent |
| `"round"` | The semicircular arc `L`→`R` of radius `r`, bulging outward |
| `"square"` | The chord `L`→`R` translated outward by `r` along the end tangent, joined to `L` and `R` by straight edges |

Joins apply at every corner of the skeleton (a vertex where the incoming and outgoing tangents differ), on the outer side of the turn:

| Join | Definition |
|---|---|
| `"miter"` | Extend both offset boundaries along their tangents to their intersection. When the miter length divided by `stroke` exceeds the miter limit, use `"bevel"` |
| `"round"` | Circular arc of radius `r` between the two boundary endpoints |
| `"bevel"` | Straight segment between the two boundary endpoints |

The miter limit is `MITER_LIMIT` = 4 (§14). `joinAt` overrides `joins` at the named segment's endpoint.

On the inner side of a corner, the join style has no effect. The incoming and outgoing offset boundaries are each cut at their crossing nearest the corner and meet at that single point (§7.4).

**Angled terminals** come from the end tangents (§6.3). The start cap is perpendicular to the first segment's departure tangent; the end cap is perpendicular to the final segment's arrival tangent. On a curve, those are set by the first and last control points:

```
path arm (stroke: hair, caps: "butt") {
  start (at: b)
  cube  (c1: polar(b, k, 0deg), c2: polar(t, k, 70deg + 180deg), to: t)   // arrives at 70°
}
```

**Contrast** between strokes exists between paths: a stem at `stem` and a crossbar at `hair` are separate paths. Contrast within one stroke is not expressible; a shape needing it is authored as a `fill` (§6.5).

### 6.5 Region assembly

A glyph renders every rendering path, in declaration order, and every component. Overlap between them is expected.

A slab serif is a second path, a short path with its own `stroke` overlapping the stem.

**Flat tops and apexes** come from an overlapping path:

```
glyph A (codepoint: U+0041, rsb: sidebear) {
  let apexY = capHeight.y - stem/2;

  path legL (stroke: stem) { start (at: (xL, apexY)) line (to: footL) }
  path legR (stroke: stem) { start (at: (xR, apexY)) line (to: footR) }
  path apexBar (stroke: stem) {
    start (at: (xL, apexY))
    line  (to: (xR, apexY))
  }
}
```

**Counters** arise three ways:
- Stroking a closed path yields an outer and a counter contour.
- Counters form between separate overlapping open paths, such as the stem and bowl of `B` or the spine and bowl of `6`: no contour covers the enclosed area, so nonzero winding leaves it empty.
- A filled contour nested inside another filled contour is a counter (§8.1).

**Filling.** `fill: true` inks the closed path's interior. The path's own skeleton is the contour; no offsetting happens, so caps, joins, and fold trimming (§7.2) do not apply to it.

```
path wedge (fill: true) {                   // wide base, rounded top
  start (at: (x0, 0))
  arc   (center: (xm, 0), to: (xm, top), sweep: "cw")
  arc   (center: (xm, 0), to: (x1, 0),   sweep: "cw")
  close                                     // the base: a line back to (x0, 0)
}

path ring (stroke: hair, fill: true) {      // filled, with a hairline border
  start (at: t)
  arc   (center: ctr, to: r, sweep: "cw")
  arc   (center: ctr, to: b, sweep: "cw")
  arc   (center: ctr, to: l, sweep: "cw")
  arc   (center: ctr, to: t, sweep: "cw")
  close
}
```

`stroke` and `fill` on one closed path produce a solid shape: the skeleton's interior grown by `stroke/2`. The stroke's counter contour lies inside the fill and does not cut a hole (§8.1).

---

## 7. Offset generation

This stage processes paths that declare `stroke`. A `fill`-only path passes straight to §8.

### 7.1 Offset accuracy

The stroke boundary is `p(t) ± r·n̂(t)` with `r = stroke/2`, plus the caps and joins of §6.4. The generated outline consists of cubic Béziers whose Hausdorff distance from that exact boundary is at most `OFFSET_TOLERANCE` (§14). The approximation method is the implementation's choice.

### 7.2 Tight curvature

Where the curvature radius inside a skeleton segment is smaller than `r`, the offset on the concave side folds back on itself: it doubles back past the curve's centre of curvature and crosses itself, leaving a small reversed loop (a swallowtail) — the same kind of inner-side self-crossing §7.4 trims at corners. Where the fold reaches all the way round a closed path, the offset also cuts a counter the pen actually fills. The joints between an `arc`'s cubic pieces are interior to that segment; corners between segments are not, and joins handle them (§6.4).

**Folds are resolved.** The stroke's ink is defined as a round pen of radius `r` swept along the skeleton, so on the concave side of a fold the ink edge is the part of the offset that stays at least `r` from the skeleton. After offset generation, every fold is trimmed:

1. A **fold interval** is a maximal parameter interval inside one segment where the curvature radius is below `r` on the side the curve turns toward.
2. The output contour crosses itself around each fold. Starting from the fold, search outward along the contour, in both directions, for the nearest crossing pair that encloses the fold's reversed piece. Cut both pieces at that crossing and drop the loop between them. The inner edge then meets itself at one point: a corner on the outline where the skeleton has none.
3. The search covers the whole contour: other segments' offsets, joins, and caps. A fold that runs into a cap is cut where the folded offset crosses the cap.
4. A closed path's counter contour (§8.1) that has no length left after trimming is dropped: the stroke has filled the counter.

Trimming never changes the outer side of the stroke. When folds overlap or adjoin, they are trimmed as one loop.

Folds produce **no diagnostic**: the stroke is the pen's ink, and a tight curve is a legitimate shape at any `stroke`.

### 7.3 Degenerate input

- A path with zero total arc length → geometry error.
- A segment whose endpoint equals its start point → geometry error. The omitted closing segment of §5.7 is exempt.
- `stroke` ≤ 0 → geometry error.
- A segment of a stroked path whose derivative vanishes at an interior point (a cusp) → geometry error. Its offset has no defined direction there.

An open path whose final point coincides with its start point is valid; it is capped at both ends.

### 7.4 Inner corners and self-overlap

**Inner corners are resolved.** At every corner, including the closing corner of a closed path, the inner offset boundaries of the two adjacent segments are cut at their crossing nearest the corner and joined there (§6.4). The outline does not loop past itself on the inner side of a turn.

The crossing must lie on the inner offsets of the two segments adjacent to the corner. It does not when the turn is sharp and an adjacent segment is too short for the stroke: the inner side would have to swallow the whole segment. This is an **error**. Report the glyph, the path, the segment the corner ends, and the instance being built.

A corner whose tangents are exactly opposite (a stroke doubling straight back on itself) has no crossing either, since the two inner offsets are exactly parallel — but this is a legitimate shape, not the error above. The two offsets are left as kurbo drew them, meeting at its own raw connector rather than a resolved single point.

**Other self-overlap is kept.** A stroke outline may still overlap itself where the skeleton crosses itself, and where two non-adjacent parts of one path pass within `stroke` of each other. This is valid output. Filled with the nonzero rule, the outline renders exactly the stroke's ink. It is neither an error nor resolved.

---

## 8. Winding

Overlaps are kept. Rasterizers fill `glyf` and CFF outlines with the nonzero winding rule, so overlapping contours of consistent direction render as their union. There is no overlap-removal option.

### 8.1 Contour roles

Every contour carries a **role**, assigned by its producer:

| Source | Contours | Roles |
|---|---|---|
| Stroked open path | one | outer |
| Stroked closed path | two | outer (the one enclosing the other), counter |
| Filled path | one | outer, unless nesting makes it a counter (below) |

Among the filled contours of one glyph's own paths and path components (§5.7), a filled contour enclosed by an odd number of other filled contours takes the counter role. Stroked contours and glyph-component contours do not take part in this count.

Consequence: a closed path with both `stroke` and `fill` yields stroke-outer (outer), the fill (outer), and stroke-counter (counter). Under nonzero winding every point inside stroke-outer has winding number ≥ 1, so the shape renders solid.

### 8.2 Winding direction

| Format | Outer | Counter |
|---|---|---|
| TrueType `glyf` (y-up) | clockwise | counter-clockwise |
| CFF / CFF2 (y-up) | counter-clockwise | clockwise |

Compute the signed area of each contour and reverse any contour whose direction disagrees with its role under the target format.

### 8.3 Self-intersecting fills

A self-intersecting filled contour is an **error**, naming the glyph, the path, and the parameter values of each crossing. It is not resolved. Detection is a curve–curve intersection test over the contour's own segments, excluding the shared endpoints of adjacent segments. Where adjacent pieces meet tangentially (an `arc`'s own cubic pieces), they stay within the solver's tolerance of each other a short way past the joint, so a hit between neighbours within `1e-3` of the shorter piece's length of their shared point is that joint, not a crossing.

---

## 9. Editor

### 9.1 Tools

Each tool that changes the design maps to a named structured edit on the syntax tree.

**Construction:**

| Tool | Writes |
|---|---|
| point | a `let` with a literal pair or a §5.9 constructor |
| line, vertical guide, symmetry axis | a `let` bound to `lineThrough` / `lineAt` / `hline` / `vline` |
| ellipse, circle | a `let` bound to `ellipse` / `circle` |
| metric guide | a `metric` declaration |
| measurement | a `let` |
| grid | nothing — grid snapping is a canvas setting and is not part of the source |

**Shape:**
- path tool: places points, appending `line`, `quad`, `cube`, or `arc`
- segment-kind toggle: among `line`, `quad`, `cube`, and `arc`, seeding the target kind's required fields from the current geometry
- control-point handles (`quad`, `cube`); centre, radius, sweep, and large-arc controls (`arc`), where dragging a radius handle on a centre-mode arc switches it to radii mode
- stroke tool: sets `stroke`, which also promotes a construction path to a rendering one
- fill toggle: sets `fill`, and appends `close` if absent
- cap tool (three-way)
- join tool (three-way, path-wide plus per-segment)
- end-direction tool: rewrites the first or last segment's control point adjacent to the end as `polar(endpoint, len, θ)`
- transform and instance placement
- component reference: a whole glyph, or a single path (this glyph's or another's)

Every tool sets a property on a single block. Switching a segment's kind removes the fields the new kind does not admit.

**Relationship tools** rewrite one definition:

| Tool | Rewrites as |
|---|---|
| Make coincident | one point's definition becomes a reference to the other |
| Snap to intersection | `let p = meet(lineThrough(a,b), lineThrough(c,d));` |
| Cast onto ellipse | `let p = cast(lineAt(c, θ), e);` |
| Project onto line | `let p = project(q, l);` |
| Place at fraction | `let p = mediate(a, b, 0.35);` |
| Make parallel / at angle | `let p = polar(q, len, θ);` |
| Mirror across axis | `let p = mirror(q, axis);` |
| Promote literal to parameter | a literal becomes a reference to a new or existing `param` |
| Make smooth | omits `c1` or `c` where §6.3 allows reflection; otherwise rewrites the control as `2·p − c′` from its neighbour across the joint |

**Diagnostics:** dependency inspector (upstream and downstream of any value), cycle reporter, unresolved-name reporter, parameter sweep preview.

### 9.2 Inverse drag

A dragged value may be derived. Its **drivers** are the numeric literals and param references upstream of it in the dependency graph.

1. **Literal value** (the dragged coordinate is itself a literal) → rewrite the literal.
2. **Exactly one driver** → find the driver value that minimizes the distance between the dragged value and the pointer, by a one-dimensional search (secant or Newton). A param driver's search is bracketed by its declared `range`; a literal driver's search is unbracketed. Rewrite that one literal, or the param's `default`. The glyph re-evaluates through its full dependency chain.
3. **More than one driver** → present the drivers and ask which to drive, then apply case 2. A per-point "last driven" memory makes repeat drags immediate.

Dragging a stroke edge edits the `stroke` expression by the same three cases.

### 9.3 Round-trip requirements

- **Lossless syntax tree:** comments, blank lines, and formatting survive any structured edit. Only touched nodes are reformatted.
- **Stable node identity:** selection, undo, and diagnostic anchors survive re-parse. Identity is assigned at parse, not taken from byte offsets.
- **Incremental evaluation:** an edit dirties a subgraph; re-evaluate only affected glyphs.
- **Partial-text tolerance:** while typing, the source is often invalid. Render the last good state plus a diagnostic, never a blank canvas.

---

## 10. Compilation to outlines

### 10.1 Components

```
glyph eacute (codepoint: U+00E9, advance: glyphs.e.advance) {
  component (glyph: e)
  component (glyph: acute,
             offset: (glyphs.e.top.x - glyphs.acute.top.x, accentGap))
}
```

Placement may be an expression over the referenced glyph's `advance`, `bbox`, and declared anchors, so accents re-centre when weight changes.

A component draws the referenced glyph's authored outline (§5.10), without that glyph's own shift `hₜ`, transformed by `M`. Its ink counts towards the host's `glyph.bbox` in the host's authored coordinates. The host's own shift `h` then applies to it as to any other ink. The referenced glyph's outline is stored placed (with `hₜ` applied), so the component is emitted with transform `T·M·Tₜ⁻¹`, where `T` = `translate(h, 0)` and `Tₜ` = `translate(hₜ, 0)`.

Component references must be acyclic, and nesting depth is at most `COMPONENT_DEPTH` = 5. Violations are export errors.

Under an instance `slant` with shear `S`, the component is emitted with transform `S·T·M·Tₜ⁻¹·S⁻¹`, so that the composite equals the slanted decomposed outline.

A component is emitted as a `glyf` composite when its transform's 2×2 part fits F2Dot14 (each entry in [−2, 2)); its offset is rounded per §10.4. Otherwise it is decomposed. CFF output always decomposes.

**Path components** are not components in the output: `glyf` composites reference whole glyphs. A path component's contours are this glyph's own, exactly as if its transformed skeleton were declared here as a `path` with the effective `stroke`, `fill`, `caps`, and `joins` (§5.7). They take part in the filled-contour role count (§8.1) like any other path's, and slant, extrema, and quantization treat them as the glyph's own outline. A path read from another glyph is in that glyph's authored coordinates (§5.10), the same as a glyph component's outline.

### 10.2 Extrema insertion

Insert on-curve points at the horizontal and vertical extrema of every contour, after slant has been applied.

### 10.3 Curve conversion

Conversion operates on unrounded coordinates.

- **CFF / CFF2 (`.otf`)** — cubics pass through directly. Charstring encoding; subroutinization is optional.
- **TrueType `glyf` (`.ttf`)** — quadratic only. Convert each cubic to a quadratic spline within `CU2QU_TOLERANCE` (§14) by recursive splitting (cu2qu), and omit on-curve points that are the exact midpoint of their neighbouring off-curve points.
- **WOFF2** — `glyf`/`loca` transformed per the WOFF2 specification, table directory Brotli-compressed. A repackaging step after TTF assembly.

### 10.4 Zone snapping and quantization

1. **Zone snap.** An on-curve point whose tangent is horizontal and whose y lies within `ZONE_SNAP_TOLERANCE` (§14) of a metric's `.y` or `.ink` takes that value, rounded.
2. **Quantize** every remaining coordinate and every component offset to an integer, rounding half away from zero.
3. Remove segments that became zero-length.
4. Re-run the §8.3 self-intersection test on filled contours. A crossing introduced by rounding is an export error.

### 10.5 Limits

`maxp` point and contour counts are `uint16`. `glyf` coordinates and deltas are int16. Check both, plus total table sizes, and report violations as export errors.

### 10.6 Glyph names, codepoints, `cmap`

- **Glyph identity is the DSL name**, not the codepoint. A glyph may carry zero codepoints or several. Glyph names are identifiers (§5.1), at most 63 bytes.
- **Glyph order:** glyph ID 0 is `.notdef`, always generated, with no contours and advance `round(font.em / 2)`. The remaining glyphs follow in declaration order (§14).
- **`cmap` subtables:** format 4 for the BMP, plus format 12 whenever any codepoint exceeds U+FFFF. Encoding records: (3,1) and (0,3) for format 4; (3,10) and (0,4) for format 12.
- **Variation sequences:** whenever any glyph declares `variation`, a format 14 subtable with encoding record (0,5). Its variation-selector records are sorted by selector, and each record's mappings by base. A sequence on the glyph its base already maps to through `codepoint` goes in the record's default-UVS ranges; every other sequence is a non-default-UVS mapping to its glyph.
- **Validation:** a codepoint on more than one glyph, a surrogate (U+D800–U+DFFF), or a noncharacter is an error. A codepoint unassigned in Unicode 16.0 is a warning.
- **Sequence validation:** a sequence on more than one glyph, a `selector` outside the variation-selector ranges (§5.6), or a `base` that is a surrogate or noncharacter is an error. A sequence with a VS1–VS16 selector that is not a standardized variation sequence or an emoji variation sequence in Unicode 16.0 (`StandardizedVariants.txt`, `emoji-variation-sequences.txt`) is a warning; VS17–VS256 sequences are ideographic variation sequences, registered outside Unicode, and are not checked. A sequence whose `base` no glyph maps through `codepoint` is a warning: text without the selector falls back to another font.
- **`post`** version 2.0 carries glyph names.

---

## 11. Hinting

### 11.1 Alignment zones (CFF)

Every `metric` is an alignment zone:

| Metric | Private DICT entry |
|---|---|
| `baseline` | first `BlueValues` pair: `(y − overshoot, y)` |
| other, `align: "top"` | `BlueValues` pair: `(y, y + overshoot)` |
| other, `align: "bottom"` | `OtherBlues` pair: `(y − overshoot, y)` |

Values are rounded per §10.4. Zones with identical bounds are merged; overlapping zones in the same array are merged into their union. `BlueValues` holds at most 7 pairs (baseline included) and `OtherBlues` at most 5. Excess zones are dropped, last-declared first, with a warning.

`BlueFuzz` = 0, `BlueShift` = 7, `BlueScale` = `min(0.039625, 0.99 / maxZoneHeight)`, where `maxZoneHeight` is the largest zone height.

### 11.2 Stem widths (CFF)

A straight skeleton segment is **vertical** when its x-extent is zero and **horizontal** when its y-extent is zero.

- `StdVW` is the `stroke` value with the greatest total length over vertical stroked segments across the font; ties go to the smaller value. `StdHW` is the same over horizontal segments.
- `StemSnapV` / `StemSnapH` list the distinct values of those sets in ascending order, at most 12; when there are more, keep the 12 with the greatest total length.

### 11.3 Per-glyph hints (CFF)

- A vertical stroked segment at x = `c` yields `vstem (c − r, c + r)`; a horizontal one at y = `c` yields `hstem (c − r, c + r)`.
- A stroked curve's x-extremum at `c` yields `vstem (c − r, c + r)`; a y-extremum yields `hstem` likewise.
- Where stems in one axis overlap, emit `hintmask` replacement.
- Filled paths yield no stem hints.

### 11.4 TrueType

TrueType output carries no glyph instructions, no `cvt`, `fpgm`, or `prep`. It carries a `gasp` table with one range, `0xFFFF`, flags `0x000F` (gridfit, grayscale, symmetric gridfit, symmetric smoothing).

---

## 12. Metrics and kerning

### 12.1 Advance widths and sidebearings

Paths are centrelines, so a stroke's ink extends `stroke/2` past them. Sidebearings are measured from ink, `glyph.bbox`, not from centrelines.

A glyph's horizontal spacing comes down to two numbers: its advance, and a horizontal **shift** `h` added to every x coordinate of its ink. The ink width, `glyph.bbox.width`, is measured from the outline and never declared. The glyph's one or two declared fields (§5.6) determine both numbers. With `b` = `glyph.bbox`:

| Declared | Shift `h` | Advance |
|---|---|---|
| `advance` | `0` | `advance` |
| `rsb` | `0` | `b.x1 + rsb` |
| `lsb` | `lsb − b.x0` | `lsb + b.width + lsb` |
| `lsb`, `rsb` | `lsb − b.x0` | `lsb + b.width + rsb` |
| `advance`, `lsb` | `lsb − b.x0` | `advance` |
| `advance`, `rsb` | `advance − rsb − b.x1` | `advance` |

When `h` is `0`, the paths are already in final position: `advance` alone keeps the ink exactly where it was authored, and `rsb` alone sets the right sidebearing from the ink as authored. `lsb` alone gives equal sidebearings.

```
glyph O (codepoint: U+004F, rsb: sidebear) { … }   // left ink authored at sidebear
glyph o (codepoint: U+006F, lsb: sidebear) { … }   // authored anywhere; equal bearings
glyph A (codepoint: U+0041, advance: cell,         // monospace: ink centred in the cell
         lsb: (cell - glyph.bbox.width) / 2) { … }
```

`glyph.bbox` is in authored coordinates (§5.10), so `lsb` and `rsb` expressions may read it without a cycle. `glyph.advance` is readable too. An `lsb` or `rsb` that reads it when the advance is derived from that same field is a cycle (§13).

The editor shows a glyph in authored coordinates, and draws its origin and advance guides at `x = −h` and `x = advance − h`. A drag is therefore never inverted through the shift: the ink stays put and the frame moves.

`hmtx` left sidebearings are read from the final quantized outline.

Vertical metrics, rounded per §10.4:

| Table field | Value |
|---|---|
| `hhea.ascender`, `OS/2.sTypoAscender` | `ascender.y` |
| `hhea.descender`, `OS/2.sTypoDescender` | `descender.y` |
| `hhea.lineGap`, `OS/2.sTypoLineGap` | `0` |
| `OS/2.sCapHeight` | `capHeight.y` |
| `OS/2.sxHeight` | `xHeight.y` |
| `OS/2.usWinAscent` | maximum `yMax` over all glyphs |
| `OS/2.usWinDescent` | maximum `−yMin` over all glyphs |
| `OS/2.fsSelection` | `USE_TYPO_METRICS` set |
| `post.underlineThickness`, `OS/2.yStrikeoutSize` | `round(font.em / 20)` |
| `post.underlinePosition` | `round(−font.em / 10)` |
| `OS/2.yStrikeoutPosition` | `round(xHeight.y / 2)` |

### 12.2 Kerning

```
group roundRight (glyphs: [ o, c, e, b, p, thorn ])
group roundLeft  (glyphs: [ o, c, e, d, q ])

kern (left: roundRight, right: roundLeft, by: -0.015em)
kern (left: A,          right: V,         by: -0.05em * kernStrength)
```

- `by` is an ordinary expression, so kerning varies per instance. It is rounded per §10.4.
- A glyph may belong to at most one group that appears as a `left`, and at most one group that appears as a `right`.
- Two `kern` declarations with the same `left` and `right` are an error.
- A glyph–glyph `kern` takes precedence over a group `kern` covering the same pair.
- A `kern` whose `left` or `right` names neither a glyph nor a group is an export error.

Compile to one `kern` feature under script `DFLT`, default language. Glyph–glyph pairs are GPOS lookup type 2 format 1; group pairs are format 2 with classes kept as classes. The format-1 subtable precedes the format-2 subtables, which implements the precedence rule.

Kerning is the only OpenType layout the language expresses, so GPOS is the only layout table generated. Substitution (GSUB), mark attachment, and all other layout features have no representation in the source.

### 12.3 Instances

```
instance Regular   ()
instance Bold      (stem: 160, contrast: 0.80, weightClass: 700)
instance Condensed (capW: 520, figW: 470, widthClass: 3)
instance Italic    (slant: 11deg, glyphset: Italic)
```

Each instance is a full independent build.

**Slant.** An instance's `slant` θ applies the shear `(x, y) → (x + y·tan θ, y)` to every outline after winding normalization (§3). Positive θ leans right. `glyph.bbox`, `advance`, and the §12.1 shift are evaluated, and the shift applied, before the shear. The shear sets:
- `post.italicAngle` = −θ in degrees
- `hhea.caretSlopeRise` = `font.em`, and `hhea.caretSlopeRun` = `round(font.em · tan θ)`

**Glyph sets.** An instance with `glyphset: S` builds, for every default-set glyph, its alternate in `S` when one exists and the default definition otherwise. Glyph names and IDs are the same across instances.

**Naming.** An instance is **RIBBI** when its `styleName` is `Regular`, `Italic`, `Bold`, or `Bold Italic`.

| Field | RIBBI | Other |
|---|---|---|
| name ID 1 | `font.name` | `font.name` + " " + `styleName` |
| name ID 2 | `styleName` | `Regular` |
| name ID 16 / 17 | absent | `font.name` / `styleName` |
| `OS/2.fsSelection` | `BOLD` / `ITALIC` / `REGULAR` per `styleName` | `REGULAR` |

`head.macStyle` mirrors the `BOLD` and `ITALIC` bits. Name ID 4 is family plus style; name ID 6 is `font.name` without spaces, "-", and `styleName` without spaces. `OS/2.usWeightClass` and `usWidthClass` come from `weightClass` and `widthClass`.

---

## 13. Error model

Every diagnostic names a source location, an entity, and where possible a fix. Diagnostics are errors unless listed as warnings.

| Class | Examples |
|---|---|
| Syntax | Unexpected token; unclosed block; malformed segment declaration; malformed range; hex integer above 2^53; codepoint above `U+10FFFF`; character literal empty, holding more than one scalar value, or with an unknown escape (§5.1) |
| Type | `pair` where `num` expected; `path` argument to a scalar function; `.bbox` on a value that has no extent; a mixed tuple; non-integral value in an `int` field |
| Field validation | Unknown field; unknown enum value, with the legal set enumerated; missing required field; mutually exclusive fields both present; a field illegal in its position (§5.7); non-constant expression where one is required; param default or instance override outside `range`; param named after an instance field; alternate glyph with no default glyph, or with `codepoint` or `variation`; a `variation` selector outside VS1–VS256; `codepoint` value outside `0`–`0x10FFFF`; unknown glyph set; a glyph declaring none, or all three, of `advance`, `lsb`, `rsb`; a `component` with both or neither of `glyph` and `path`; `stroke`, `fill`, `caps`, `joins`, or `joinAt` on a glyph component; a path component with neither `stroke` nor `fill` after its overrides; `fill` on a path component whose path is open |
| Name resolution | Unresolved identifier with scope and near-miss suggestions; duplicate definition; shadowing a top-level name; reserved word as a declaration name; anchor named `advance` or `bbox` |
| Cycle | Circular definition, reported as the full cycle path with file and line per hop, plus a suggested edge to break (§4.3) |
| Domain | `sqrt` of a negative; division by zero; `asin`/`acos` out of range; `meet` on parallel lines; `lineThrough` of two equal points; an `ellipse` or `circle` radius ≤ 0; `cast` with no crossing ahead of the line's origin; path parameter out of domain; `minOf`/`maxOf` of an empty list; `.bbox` of a glyph with no ink |
| Geometry | Zero-length path or segment; a centre-mode `arc` whose endpoints admit no axis-aligned ellipse about its `center`, or a radii-mode `arc` whose chord is longer than its radii can span (§6.3); `stroke` ≤ 0; an interior cusp in a stroked segment (§7.3); a corner whose inner offsets do not cross within its two adjacent segments (§7.4), naming glyph, path, segment, and instance; a self-intersecting filled contour (§8.3) |
| Path structure | Every structural error of §6.3 |
| Metrics | A missing reserved metric; `baseline.y` ≠ 0; negative `overshoot` |
| Export | Point or contour count over `maxp` limits; coordinate out of int16 range; `kern` or `group` naming an undefined glyph; duplicate, surrogate, or noncharacter codepoint; duplicate variation sequence, or one with a surrogate or noncharacter base; component cycle or excessive depth; self-intersection introduced by quantization; kerning group overlap (§12.2) |
| **Warnings** | `caps` on a closed path (§5.7); alignment zones dropped (§11.1); codepoint unassigned in Unicode 16.0; a VS1–VS16 sequence that Unicode 16.0 does not standardize; a sequence whose base no glyph encodes (§10.6) |

---

## 14. Determinism

Goal: byte-identical output from identical source files, in identical order, built by the same compiler binary on the same target platform.

- `f64` throughout; no fast-math or reassociation.
- Topological ties are broken by declaration index: file position in the input list, then position within the file. Never by hash or discovery order.
- Iteration order everywhere is declaration order or sorted keys.
- Rounding is half away from zero wherever this document says "rounded".
- `head.created` / `head.modified` are settable to a fixed value; table order and padding are fixed.

Named constants. Tolerances scale with the em size.

| Constant | Value |
|---|---|
| `OFFSET_TOLERANCE` | `0.05 · font.em / 1000` |
| `CU2QU_TOLERANCE` | `0.5 · font.em / 1000` |
| `ZONE_SNAP_TOLERANCE` | `1 · font.em / 1000` |
| `ARC_TOLERANCE` | `0.01 · font.em / 1000` |
| `MITER_LIMIT` | `4` |
| `COMPONENT_DEPTH` | `5` |

---

## 15. Verification

1. **Evaluation engine**
   - Property test: permuting statement order within a scope yields identical results.
   - Cycle detection covering self-reference, two-node, and long cycles, asserting the reported path is the actual cycle.
   - Golden tests on `meet` / `mediate` / `project` / `polar` / `mirror` against hand-computed geometry.
   - `crossings`, `cast`, and `along` on circles and ellipses against hand-computed geometry: a secant from outside, a ray from the centre at each quadrant angle, a ray from a point on the ellipse (finds the far side), a tangent line (one crossing), a miss (empty; `cast` is a domain error), and a ray pointing away from the ellipse (`cast` is a domain error; `crossings` has two negative values).
2. **Segments**
   - A `quad`'s elevated cubic evaluates identically to the quadratic.
   - An `arc`'s pieces stay within `3e-4 · max(rx, ry)` of the exact ellipse (the 90°-piece bound), checked by dense sampling; quarter, half, and three-quarter arcs in both sweeps.
   - Centre mode: the radius solve, the circular fallback on a singular system, and the geometry errors for a non-circular singular system and for no ellipse.
   - Radii mode: both `large` values in both sweeps pick the stated candidate centre; the exact-diameter chord (half-oval) solves within tolerance; a chord too long for the radii is an error, and a non-positive radius is an error.
   - Reflection produces a tangent-continuous joint; an omitted control after a segment of another kind is a structural error.
   - `close` appends a straight line, and appends nothing when the path already ends at its start.
3. **Offsets**
   - Hausdorff distance against a densely sampled exact boundary, at most `OFFSET_TOLERANCE`.
   - Caps and joins match §6.4: `"butt"` perpendicular to the end tangent; `"square"` extended by `r`; `"round"` a semicircle of radius `r`; `"miter"` falling back to `"bevel"` past `MITER_LIMIT`; `joinAt` overriding `joins` at one vertex only.
   - Each degenerate case of §7.3 produces its named error.
4. **Differential test against an independent SVG stroker** — same path, `stroke`, caps, and joins. Rasterize both outlines and assert the coverage difference stays within the area of a `2 · OFFSET_TOLERANCE` band along the boundary.
5. **Fold trimming** — fuzz random paths against random stroke widths. Assert that the trimmed outline matches the swept pen: rasterize it and the union of discs of radius `r` densely along the skeleton (plus caps and joins), and require the coverage difference to stay within a `2 · OFFSET_TOLERANCE` band along the boundary. Golden cases: an ellipse arc tighter than `r` at its vertex (the inner edge comes to a point); a fold reaching an open path's round cap; a closed ellipse whose counter closes up entirely.
6. **Path components** — a path component of a path in the same glyph, with `transform: identity`, yields contours identical to the path's own; one with `reflect(vline(x))` yields the mirror image with the same stroke width; one under `scale(2)` keeps the declared `stroke` width rather than doubling it; one overriding `stroke` on a construction path renders it; `glyphs.o.bowl` reads in `o`'s authored coordinates, unaffected by `o`'s `lsb`; a glyph component of a glyph with an `lsb` is drawn without that glyph's shift, both decomposed and as a `glyf` composite.
7. **Fills and contour roles**
   - A filled closed path produces its skeleton as one contour.
   - A fill nested in a fill produces a hole.
   - `stroke` and `fill` on one closed path produce a solid shape, asserted by rasterizing it and the stroke-outer contour alone and comparing coverage.
   - Fuzzed self-intersecting filled contours each trigger §8.3's error.
8. **Export**
   - `fonttools ttx` round-trip, `ots-sanitize`, and FontBakery (OpenType profile) in CI.
   - `cmap` format 14: a sequence on a glyph without `codepoint` is a non-default-UVS mapping; a sequence on the glyph its base encodes is a default-UVS range; records sorted by selector and base; HarfBuzz shapes `0030 FE00` to the `variation` glyph and `0030` alone to the `codepoint` one.
   - Render a pangram at 8–48 ppem with FreeType and diff against golden rasters.

Acceptance test: build a typeface covering uppercase, lowercase, digits, and basic punctuation in three weights from one source, install it, and set text in it.

---

## 16. Implementation plans

1. **DSL surface** — grammar, lexer, lossless syntax tree, AST lowering, name resolution, static type checking, stable node identity, formatter. Written against §5. Read first: per-field validation of string-valued enums (§5.5) and element-typed tuples (§5.8).
2. **Evaluation engine** — dependency graph construction from name references, topological evaluation, cycle detection with full-path reporting, the construction library, failure containment, incremental re-evaluation. (§4, §5.9, §13)
3. **Geometry kernel** — path segments (lines, quadratic and cubic Béziers, elliptical arcs), constant-width offset generation with caps and joins, fold trimming, filled contours and their self-intersection check, contour roles and winding normalization. (§6, §7, §8)
4. **Font compiler** — slant, extrema insertion, curve conversion, zone snapping and quantization, table assembly for TTF/OTF/WOFF2, hinting, metrics, kerning, instances. (§10, §11, §12, §14)
5. **Editor projection layer** — structured edits per tool, inverse drag, dependency inspection, partial-text tolerance, incremental redraw. (§9)

Plans 1 and 2 gate the rest. Plan 2 depends on plan 3's stroking for `.bbox` of rendering paths.

---

## Appendix A — Sample source

Sixteen glyphs in the full syntax: `A`–`F`, `0`–`9`, in three instances. This is the conformance target for plan §16.1.

The sample is monolinear, so it exercises `stroke` and not `fill`; §6.5 carries the filled examples.

Conventions used throughout (the language does not enforce them):
- `w` is a glyph's centreline extent.
- `ox` is the centreline x of a stroke whose left ink edge sits at `sidebear`.
- A stroke whose outer edge must touch a metric is centred half a stroke inside it: `capHeight.y - hair/2` for a flat bar, `figHeight.ink - stem/2` for a round top.
- Every glyph but `nine` declares `rsb: sidebear`, with its left ink authored at `sidebear` (§12.1).

```
// ══ Metaglyph Sans ═══════════════════════════════════════════════════
// Sample source: A–F, 0–9

font (name: "Metaglyph Sans", em: 1000)

// ── design space ─────────────────────────────────────────────────────
param stem     (default: 100,  range: 20..260)
param contrast (default: 0.86, range: 0.40..1.00)   // hair/stem ratio
param sidebear (default: 44,   range: 0..140)
param capW     (default: 620,  range: 380..900)     // nominal cap width
param figW     (default: 560,  range: 360..820)     // nominal figure width
param barPos   (default: 0.46, range: 0.30..0.62)   // crossbar height ratio

let hair = stem * contrast;
let ox   = sidebear + stem/2;

// ── vertical metrics ─────────────────────────────────────────────────
metric baseline  (y: 0,    overshoot: 10, align: "bottom")
metric xHeight   (y: 520,  overshoot: 10)
metric capHeight (y: 700,  overshoot: 12)
metric figHeight (y: 700,  overshoot: 12)
metric ascender  (y: 740)
metric descender (y: -220, align: "bottom")

// ── instances ────────────────────────────────────────────────────────
instance Regular   ()
instance Bold      (stem: 160, contrast: 0.80, weightClass: 700)
instance Condensed (capW: 520, figW: 470, widthClass: 3)

// ══ Capitals ═════════════════════════════════════════════════════════

glyph A (codepoint: U+0041, rsb: sidebear) {
    let w     = capW;
    let apexY = capHeight.y - stem/2;
    let al    = (ox + w/2 - stem/2, apexY);
    let ar    = (ox + w/2 + stem/2, apexY);
    let lf    = (ox, 0);
    let rf    = (ox + w, 0);
    let barY  = capHeight.y * barPos * 0.80;

    path legL (stroke: stem) { start (at: al) line (to: lf) }
    path legR (stroke: stem) { start (at: ar) line (to: rf) }

    path apexBar (stroke: stem) {
        start (at: al)
        line  (to: ar)
    }
    path bar (stroke: hair) {
        start (at: meet(lineThrough(al, lf), hline(barY)))
        line  (to: meet(lineThrough(ar, rf), hline(barY)))
    }
}

glyph B (codepoint: U+0042, rsb: sidebear) {
    let w   = capW * 0.88;
    let sx  = ox;
    let top = capHeight.y;
    let mid = top * 0.53;
    let yU  = (top - stem/2 + mid) / 2;   // centre heights of the two bowls
    let yL  = (mid + stem/2) / 2;

    path upright (stroke: stem) {
        start (at: (sx, top))
        line  (to: (sx, 0))
    }
    path bowlU (stroke: stem) {
        start (at: (sx, top - stem/2))
        arc   (center: (sx, yU), to: (sx + w - stem, yU), sweep: "cw")
        arc   (center: (sx, yU), to: (sx, mid),           sweep: "cw")
    }
    path bowlL (stroke: stem) {
        start (at: (sx, mid))
        arc   (center: (sx, yL), to: (sx + w - stem/2, yL), sweep: "cw")
        arc   (center: (sx, yL), to: (sx, stem/2),          sweep: "cw")
    }
}

glyph C (codepoint: U+0043, rsb: sidebear) {
    let w   = capW;
    let cy  = capHeight.y / 2;
    let hk  = w * 0.18;                              // terminal handle length
    let tU  = (ox + w * 0.93, capHeight.y * 0.79);   // upper terminal
    let tL  = (ox + w * 0.93, capHeight.y * 0.21);   // lower terminal
    let top = (ox + w/2, capHeight.ink - stem/2);
    let bot = (ox + w/2, baseline.ink + stem/2);

    path bowl (
        stroke: stem,
        caps:  "butt",
    ) {
        start (at: tU)
        cube  (c1: polar(tU, hk, 152deg), c2: polar(top, hk, 0deg), to: top)
        arc   (center: (ox + w/2, cy), to: (ox, cy), sweep: "ccw")
        arc   (center: (ox + w/2, cy), to: bot,      sweep: "ccw")
        cube  (c1: polar(bot, hk, 0deg), c2: polar(tL, hk, 208deg), to: tL)   // arrives at 28°
    }
}

glyph D (codepoint: U+0044, rsb: sidebear) {
    let w   = capW * 0.94;
    let sx  = ox;
    let top = capHeight.y;

    path upright (stroke: stem) {
        start (at: (sx, top))
        line  (to: (sx, 0))
    }
    path bowl (stroke: stem) {
        start (at: (sx, top - stem/2))
        arc   (center: (sx, top/2), to: (sx + w - stem/2, top/2), sweep: "cw")
        arc   (center: (sx, top/2), to: (sx, stem/2),             sweep: "cw")
    }
}

glyph E (codepoint: U+0045, rsb: sidebear) {
    let w   = capW * 0.80;
    let top = capHeight.y;

    path upright (stroke: stem) {
        start (at: (ox, top))
        line  (to: (ox, 0))
    }
    path barT (stroke: hair) {            // top edge on capHeight
        start (at: (sidebear, top - hair/2))
        line  (to: (sidebear + w, top - hair/2))
    }
    path barM (stroke: hair) {
        start (at: (sidebear, top * barPos))
        line  (to: (sidebear + w * 0.86, top * barPos))
    }
    path barB (stroke: hair) {            // bottom edge on baseline
        start (at: (sidebear, hair/2))
        line  (to: (sidebear + w, hair/2))
    }
}

glyph F (codepoint: U+0046, rsb: sidebear) {
    let w   = capW * 0.76;
    let top = capHeight.y;

    path upright (stroke: stem) {
        start (at: (ox, top))
        line  (to: (ox, 0))
    }
    path barT (stroke: hair) {
        start (at: (sidebear, top - hair/2))
        line  (to: (sidebear + w, top - hair/2))
    }
    path barM (stroke: hair) {
        start (at: (sidebear, top * barPos))
        line  (to: (sidebear + w * 0.86, top * barPos))
    }
}

// ══ Figures ══════════════════════════════════════════════════════════

glyph zero (codepoint: U+0030, rsb: sidebear) {
    let w   = figW * 0.86;
    let cy  = figHeight.y / 2;
    let ctr = (ox + w/2, cy);
    let top = (ox + w/2, figHeight.ink - stem/2);

    path bowl (stroke: stem) {
        start (at: top)
        arc   (center: ctr, to: (ox + w, cy),                      sweep: "cw")
        arc   (center: ctr, to: (ox + w/2, baseline.ink + stem/2), sweep: "cw")
        arc   (center: ctr, to: (ox, cy),                          sweep: "cw")
        arc   (center: ctr, to: top,                               sweep: "cw")
        close
    }
}

glyph one (codepoint: U+0031, rsb: sidebear) {
    let w   = figW * 0.54;
    let sx  = sidebear + hair/2 + w * 0.46;
    let top = figHeight.y;

    path upright (stroke: stem) {
        start (at: (sx, top))
        line  (to: (sx, 0))
    }
    path flag (stroke: hair) {
        start (at: (sx, top))
        line  (to: (sx - w * 0.46, top * 0.84))
    }
}

glyph two (codepoint: U+0032, rsb: sidebear) {
    let w    = figW;
    let top  = figHeight.y;
    let turn = (ox + w * 0.93, top * 0.66);

    path bowl (stroke: stem) {
        start (at: (ox, top * 0.78))
        arc   (center: (ox + w/2, top * 0.78), to: (ox + w/2, figHeight.ink - stem/2), sweep: "cw")
        arc   (center: (ox + w/2, turn.y),     to: turn,                               sweep: "cw")
    }
    path diag (stroke: stem) {
        start (at: turn)
        line  (to: (ox + hair * 0.6, hair * 0.6))
    }
    path base (stroke: hair) {
        start (at: (ox, hair/2))
        line  (to: (ox + w, hair/2))
    }
}

glyph three (codepoint: U+0033, rsb: sidebear) {
    let w   = figW * 0.90;
    let top = figHeight.y;
    let mid = (ox + w * 0.44, top * 0.52);           // where the two bowls meet
    let tp  = (ox + w/2, figHeight.ink - stem/2);
    let yU  = (tp.y + mid.y) / 2;                    // upper bowl's right extreme

    path bowlU (stroke: stem) {
        start (at: (ox, top * 0.80))
        arc   (center: (tp.x, top * 0.80), to: tp,                   sweep: "cw")
        arc   (center: (tp.x, yU),         to: (ox + w * 0.88, yU), sweep: "cw")
        arc   (center: (mid.x, yU),        to: mid,                  sweep: "cw")
    }
    path bowlL (stroke: stem) {
        start (at: mid)
        arc   (center: (mid.x, top * 0.26), to: (ox + w, top * 0.26), sweep: "cw")
        arc   (center: (ox, top * 0.26),    to: (ox, top * 0.14),     sweep: "cw")
    }
}

glyph four (codepoint: U+0034, rsb: sidebear) {
    let w    = figW;
    let top  = figHeight.y;
    let barY = top * 0.28;
    let ax   = sidebear + w * 0.72;

    path diag (stroke: stem) {
        start (at: (ax, top))
        line  (to: (sidebear, barY))
    }
    path bar (stroke: hair) {
        start (at: (sidebear, barY))
        line  (to: (sidebear + w, barY))
    }
    path upright (stroke: stem) {
        start (at: (ax, top))
        line  (to: (ax, 0))
    }
}

glyph five (codepoint: U+0035, rsb: sidebear) {
    let w    = figW * 0.88;
    let top  = figHeight.y;
    let neck = (ox, top * 0.56);

    path barT (stroke: hair) {
        start (at: (ox, top - hair/2))
        line  (to: (sidebear + w, top - hair/2))
    }
    path spine (stroke: stem) {
        start (at: (ox, top))
        line  (to: neck)
    }
    path bowl (stroke: stem) {
        start (at: neck)
        arc   (center: (neck.x, top * 0.28),   to: (sidebear + w, top * 0.28), sweep: "cw")
        arc   (center: (sidebear, top * 0.28), to: (sidebear, top * 0.10),     sweep: "cw")
    }
}

glyph six (codepoint: U+0036, rsb: sidebear) {
    let w  = figW * 0.88;
    let cy = figHeight.y * 0.30;
    let by = cy * 2;
    let hk = w * 0.18;                                 // terminal handle length
    let tm = (ox + w * 0.88, figHeight.y * 0.86);      // terminal
    let tp = (ox + w * 0.40, figHeight.ink - stem/2);
    let bc = (ox + w/2, cy);                           // bowl centre

    path spine (stroke: stem) {
        start (at: tm)
        cube  (c1: polar(tm, hk, 160deg), c2: polar(tp, hk, 0deg), to: tp)
        arc   (center: (tp.x, cy), to: (ox, cy), sweep: "ccw")
    }
    path bowl (stroke: stem) {
        start (at: (ox, cy))
        arc   (center: bc, to: (ox + w/2, baseline.ink + stem/2), sweep: "ccw")
        arc   (center: bc, to: (ox + w, cy),                      sweep: "ccw")
        arc   (center: bc, to: (ox + w/2, by),                    sweep: "ccw")
        arc   (center: bc, to: (ox, cy),                          sweep: "ccw")
        close
    }
}

glyph seven (codepoint: U+0037, rsb: sidebear) {
    let w   = figW * 0.92;
    let top = figHeight.y;

    path bar (stroke: hair) {
        start (at: (sidebear, top - hair/2))
        line  (to: (sidebear + w, top - hair/2))
    }
    path diag (stroke: stem) {
        start (at: (sidebear + w - hair/2, top))
        line  (to: (sidebear + w * 0.28, 0))
    }
}

glyph eight (codepoint: U+0038, rsb: sidebear) {
    let w     = figW * 0.86;
    let waist = figHeight.y * 0.53;
    let uw    = w * 0.84;
    let ux    = ox + (w - uw) / 2;
    let uy    = (waist + figHeight.y) / 2;

    let tp    = (ox + w/2, figHeight.ink - stem/2);
    let cU    = (ox + w/2, uy);
    let cL    = (ox + w/2, waist/2);

    path bowlU (stroke: stem) {
        start (at: tp)
        arc   (center: cU, to: (ux + uw, uy),     sweep: "cw")
        arc   (center: cU, to: (ox + w/2, waist), sweep: "cw")
        arc   (center: cU, to: (ux, uy),          sweep: "cw")
        arc   (center: cU, to: tp,                sweep: "cw")
        close
    }
    path bowlL (stroke: stem) {
        start (at: (ox + w/2, waist))
        arc   (center: cL, to: (ox + w, waist/2),                 sweep: "cw")
        arc   (center: cL, to: (ox + w/2, baseline.ink + stem/2), sweep: "cw")
        arc   (center: cL, to: (ox, waist/2),                     sweep: "cw")
        arc   (center: cL, to: (ox + w/2, waist),                 sweep: "cw")
        close
    }
}

glyph nine (codepoint: U+0039, advance: glyphs.six.advance) {
    // 6 rotated about the origin, then moved back so its ink spans
    // [sidebear, advance - sidebear] and its overshoots swap ends.
    component (glyph: six,
               transform: (rotate(180deg),
                           translate(glyphs.six.advance,
                                     figHeight.ink + baseline.ink)))
}
```
