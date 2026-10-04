; Metaglyph highlights (plan 4, Z2). Uses Zed's standard capture names so
; every theme colours them; see plan 4's Z2 table for the source mapping.
;
; Query order matters: later patterns override earlier ones when they
; capture the exact same node. The general call/string rules below are
; deliberately followed by more specific overrides.

(comment) @comment

; ── Literals (spec §5.1) ──────────────────────────────────────────────
(number) @number
(hex_integer) @number
(codepoint_integer) @number
(unit) @type
(character_literal) @string.special
(string) @string
(escape_sequence) @string.escape
(boolean) @boolean

; ── Namespaces and constants (spec §5.4, §5.10) ────────────────────────
(namespace) @namespace
(constant) @constant.builtin

; ── Punctuation and operators (spec §5.1, §5.8) ────────────────────────
["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," ";" ":" "."] @punctuation.delimiter
(range ".." @operator)
(unary_expression operator: "not" @keyword.operator)
(unary_expression operator: "-" @operator)
(binary_expression operator: ["and" "or"] @keyword.operator)
(binary_expression
  operator: ["^" "*" "/" "+" "-" "<" "<=" ">" ">=" "==" "!="] @operator)

; ── Calls (spec §5.9, §5.11 resolution rule 1) ─────────────────────────
(call_expression function: (identifier) @function)
(call_expression
  function: (identifier) @function.builtin
  (#any-of? @function.builtin
    "abs" "sign" "floor" "ceil" "round" "sqrt" "exp" "log"
    "sin" "cos" "tan" "asin" "acos" "atan2" "min" "max" "clamp" "lerp"
    "length" "angle" "unit" "dir" "dot" "cross" "perpendicular"
    "meet" "mediate" "project" "polar" "mirror"
    "lineThrough" "lineAt" "hline" "vline"
    "ellipse" "circle" "crossings" "along" "cast"
    "translate" "rotate" "scale" "slant" "reflect" "apply"
    "pointAt" "directionAt" "curvatureAt" "arcLength" "pointAtLength"
    "intersect" "subpath" "reverse" "extrema"
    "sum" "minOf" "maxOf"))

; ── Field names and members ─────────────────────────────────────────────
(field name: (identifier) @property)
(map_entry key: (identifier) @property)
(member_expression property: (identifier) @property)

; ── Declaration and segment keywords (spec §5.4) ────────────────────────
(let_statement "let" @keyword)
(declaration
  kind: ["font" "param" "metric" "glyph" "instance" "group" "kern"
         "path" "anchor" "component"] @keyword)
(declaration kind: ["start" "line" "quad" "cube" "arc" "close"] @function.builtin)

; ── Declaration names (spec §5.11 namespaces) ───────────────────────────
(declaration kind: ["glyph" "group"] name: (identifier) @type)
(declaration kind: ["param" "metric"] name: (identifier) @constant)
(let_statement name: (identifier) @variable)
(declaration
  kind: ["path" "anchor" "instance" "start" "line" "quad" "cube" "arc"]
  name: (identifier) @variable)

; ── Enum-valued fields (spec §5.5): overrides the generic @string above.
; @property is reused (not a fresh capture) as the predicate target, so
; the field name still renders as @property instead of leaking a
; predicate-only capture into the highlight output.
(field
  name: (identifier) @property
  value: (string) @string.special.symbol
  (#any-of? @property "caps" "joins" "align" "sweep"))
(field
  name: (identifier) @property
  value: (tuple (string) @string.special.symbol (string) @string.special.symbol)
  (#eq? @property "caps"))
(field
  name: (identifier) @property
  value: (map (map_entry value: (string) @string.special.symbol))
  (#eq? @property "joinAt"))
