// Tree-sitter grammar for Metaglyph (plan 4, Z1).
//
// This grammar follows spec §5.1-§5.2 and §5.8, but only as far as
// highlighting, indentation, and outline need: it parses every valid
// Metaglyph program, but it is not required to reject every invalid one.
// Field schemas, types, and name resolution stay in the compiler
// (`mg-syntax`'s rowan parser is canonical; this grammar is display-only).

// Binding order, tightest first, mirrors spec §5.8's numbered levels
// (level 1 highest) but as ascending tree-sitter precedence numbers
// (higher number binds tighter).
const PREC = {
  or: 1,
  and: 2,
  eq: 3,
  rel: 4,
  add: 5,
  mul: 6,
  unary: 7,
  pow: 8,
  postfix: 9, // `.` member access and `(` call (spec §5.8 level 1)
};

// Spec §5.4: declaration keywords, minus `let` (its own statement form).
const DECLARATION_KINDS = [
  "font",
  "param",
  "metric",
  "glyph",
  "instance",
  "group",
  "kern",
  "path",
  "anchor",
  "component",
  "start",
  "line",
  "quad",
  "cube",
  "arc",
  "close",
];

// Spec §5.4: namespace roots. `font`, `glyph`, and `instance` double as
// declaration keywords; `glyphs` and `math` are namespace-only.
const NAMESPACE_ROOTS = ["font", "glyph", "glyphs", "instance", "math"];

// Spec §5.10: built-in constants.
const BUILTIN_CONSTANTS = ["up", "down", "left", "right", "identity"];

module.exports = grammar({
  name: "metaglyph",

  extras: ($) => [/\s/, $.comment],

  // Enables tree-sitter's keyword-extraction: any literal string used
  // elsewhere in the grammar that matches this pattern is lexed as that
  // keyword only when the parser state actually expects it, and as a
  // plain identifier everywhere else. This is what lets `font`, `glyph`,
  // and `instance` serve as declaration keywords AND namespace roots
  // (spec §5.4) without a separate lexer mode.
  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat(choice($.let_statement, $.declaration)),

    comment: (_$) => token(seq("//", /[^\n]*/)),

    // ── Lexical (spec §5.1) ────────────────────────────────────────
    identifier: (_$) => /[A-Za-z_][A-Za-z0-9_]*/,

    number: (_$) => /[0-9]+(\.[0-9]+)?/,

    // `token.immediate` forbids whitespace before the suffix, so
    // `2 deg` (with a space) does not match (plan 4, Z1).
    unit: (_$) => token.immediate(choice("deg", "rad", "em", "%")),

    suffixed_number: ($) => seq($.number, $.unit),

    hex_integer: (_$) => /0[xX][0-9a-fA-F]+/,

    codepoint_integer: (_$) => /[Uu]\+[0-9a-fA-F]{4,6}/,

    // Shared by strings and character literals; the compiler enforces
    // which specific escapes are legal in which context (spec §5.1).
    escape_sequence: (_$) => token(seq("\\", choice("'", '"', "\\", "n", "t"))),

    character_literal: ($) =>
      seq("'", choice($.escape_sequence, token.immediate(/[^'\\]/)), "'"),

    string: ($) =>
      seq(
        '"',
        repeat(choice($.escape_sequence, token.immediate(/[^"\\]+/))),
        '"',
      ),

    boolean: (_$) => choice("true", "false"),

    // ── Namespaces and constants (spec §5.4, §5.10) ─────────────────
    namespace: (_$) => choice(...NAMESPACE_ROOTS),

    constant: (_$) => choice(...BUILTIN_CONSTANTS),

    // ── Structure (spec §5.2) ────────────────────────────────────────
    let_statement: ($) =>
      seq(
        "let",
        field("name", $.identifier),
        "=",
        field("value", $._expression),
        ";",
      ),

    // One generic rule for all 16 non-`let` declaration keywords, so the
    // grammar never lags a spec change to a field list (plan 4, Z1).
    declaration: ($) =>
      seq(
        field("kind", choice(...DECLARATION_KINDS)),
        optional(field("name", $.identifier)),
        optional(field("config", $.config)),
        optional(field("body", $.body)),
      ),

    config: ($) =>
      seq(
        "(",
        optional(seq($.field, repeat(seq(",", $.field)), optional(","))),
        ")",
      ),

    field: ($) =>
      seq(field("name", $._field_name), ":", field("value", $._field_value)),

    // `component`'s `glyph:` field reuses the `glyph` keyword as a field
    // name (spec §5.7); aliasing it to `identifier` keeps field names
    // one node type for queries (plan 4, Z1).
    _field_name: ($) => choice($.identifier, alias("glyph", $.identifier)),

    _field_value: ($) => choice($._expression, $.range),

    body: ($) => seq("{", repeat(choice($.let_statement, $.declaration)), "}"),

    // Only `param`'s `range:` field uses this syntax; it is not a
    // general expression (spec §5.2, §5.6).
    range: ($) => seq(field("start", $._bound), "..", field("end", $._bound)),

    _bound: ($) => seq(optional("-"), choice($.number, $.suffixed_number)),

    // ── Expressions (spec §5.8) ──────────────────────────────────────
    _expression: ($) =>
      choice(
        $.number,
        $.suffixed_number,
        $.hex_integer,
        $.codepoint_integer,
        $.character_literal,
        $.string,
        $.boolean,
        $.constant,
        $.namespace,
        $.identifier,
        $.parenthesized_expression,
        $.tuple,
        $.list,
        $.map,
        $.call_expression,
        $.member_expression,
        $.unary_expression,
        $.binary_expression,
      ),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    // Two or more elements (spec §5.8); a single parenthesized element
    // is `parenthesized_expression` above.
    tuple: ($) =>
      seq(
        "(",
        $._expression,
        repeat1(seq(",", $._expression)),
        optional(","),
        ")",
      ),

    list: ($) =>
      seq(
        "[",
        optional(
          seq($._expression, repeat(seq(",", $._expression)), optional(",")),
        ),
        "]",
      ),

    map: ($) =>
      seq(
        "{",
        optional(seq($.map_entry, repeat(seq(",", $.map_entry)), optional(","))),
        "}",
      ),

    map_entry: ($) =>
      seq(
        field("key", choice($.identifier, $.string)),
        ":",
        field("value", $._expression),
      ),

    // Resolution rule 1 (spec §5.11): a bare identifier followed by `(`
    // resolves only against §5.9 functions, so the callee is always a
    // plain identifier, never a member chain.
    call_expression: ($) =>
      prec(
        PREC.postfix,
        seq(field("function", $.identifier), "(", optional($.argument_list), ")"),
      ),

    argument_list: ($) =>
      seq($._expression, repeat(seq(",", $._expression)), optional(",")),

    member_expression: ($) =>
      prec(
        PREC.postfix,
        seq(field("object", $._expression), ".", field("property", $.identifier)),
      ),

    unary_expression: ($) =>
      prec(
        PREC.unary,
        seq(field("operator", choice("-", "not")), field("operand", $._expression)),
      ),

    binary_expression: ($) =>
      choice(
        prec.right(PREC.pow, seq(field("left", $._expression), field("operator", "^"), field("right", $._expression))),
        ...[
          ["*", PREC.mul],
          ["/", PREC.mul],
        ].map(([op, p]) =>
          prec.left(p, seq(field("left", $._expression), field("operator", op), field("right", $._expression))),
        ),
        ...[
          ["+", PREC.add],
          ["-", PREC.add],
        ].map(([op, p]) =>
          prec.left(p, seq(field("left", $._expression), field("operator", op), field("right", $._expression))),
        ),
        ...[
          ["<", PREC.rel],
          ["<=", PREC.rel],
          [">", PREC.rel],
          [">=", PREC.rel],
        ].map(([op, p]) =>
          prec.left(p, seq(field("left", $._expression), field("operator", op), field("right", $._expression))),
        ),
        ...[
          ["==", PREC.eq],
          ["!=", PREC.eq],
        ].map(([op, p]) =>
          prec.left(p, seq(field("left", $._expression), field("operator", op), field("right", $._expression))),
        ),
        prec.left(PREC.and, seq(field("left", $._expression), field("operator", "and"), field("right", $._expression))),
        prec.left(PREC.or, seq(field("left", $._expression), field("operator", "or"), field("right", $._expression))),
      ),
  },
});
