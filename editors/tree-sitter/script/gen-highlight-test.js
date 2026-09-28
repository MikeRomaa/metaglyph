// One-off generator for test/highlight/basics.mg (plan 4, Z2). Columns
// are computed from substring lookup, not hand-counted, so caret
// alignment can't silently drift when a line is edited. Regenerate with:
//   node script/gen-highlight-test.js > test/highlight/basics.mg
const blocks = [
  {
    code: 'font (name: "Metaglyph Sans", em: 1000)',
    marks: [
      ["nt", "keyword"], // inside "font"; a line-initial token can't start at column < 2
      ["name", "property"],
      ["Metaglyph", "string"],
      ["em", "property"],
      ["1000", "number"],
    ],
  },
  {
    code: "param stem (default: 100, range: 20..260)",
    marks: [
      ["ram", "keyword"], // inside "param"
      ["stem", "constant"],
      ["default", "property"],
      ["100", "number"],
      ["range", "property"],
      ["20", "number"],
      ["..", "operator"],
    ],
  },
  {
    code: "let hair = stem * contrast;",
    marks: [
      ["t ", "keyword"], // inside "let"
      ["hair", "variable"],
      ["*", "operator"],
    ],
  },
  {
    code: "glyph six (advance: glyph.bbox.x1 + sidebear) {",
    marks: [
      ["yph", "keyword"], // inside "glyph"
      ["six", "type"],
      ["advance", "property"],
      ["glyph.bbox", "namespace"],
      ["bbox", "property"],
    ],
  },
  {
    code: "  anchor top (at: (1, 2))",
    marks: [
      ["anchor", "keyword"],
      ["top", "variable"],
      ["at", "property"],
    ],
  },
  {
    code: '  path p (stroke: hair, caps: "butt") {',
    marks: [
      ["path", "keyword"],
      ["p (", "variable"],
      ["stroke", "property"],
      ["caps", "property"],
      ["butt", "string.special.symbol"],
    ],
  },
  { code: "    start (at: (0, 0))", marks: [["start", "function.builtin"]] },
  { code: "    line (to: (1, 1))", marks: [["line", "function.builtin"]] },
  { code: "    close", marks: [["close", "function.builtin"]] },
  { code: "  }", marks: [] },
  {
    code: "  component (glyph: six, transform: identity)",
    marks: [
      ["component", "keyword"],
      ["glyph", "property"],
      ["identity", "constant.builtin"],
    ],
  },
  { code: "}", marks: [] },
  {
    code: "instance Bold (stem: 160, slant: 0deg)",
    marks: [
      ["stance", "keyword"], // inside "instance"
      ["Bold", "variable"],
      ["stem", "property"],
      ["deg", "type"],
    ],
  },
  {
    code: "kern (left: A, right: V, by: -20)",
    marks: [
      ["rn", "keyword"], // inside "kern"
      ["left", "property"],
    ],
  },
  {
    code: "let round_trip = sqrt(2) * up.x;",
    marks: [
      ["round_trip", "variable"],
      ["sqrt", "function.builtin"],
      ["up", "constant.builtin"],
      [".x", "property"],
    ],
  },
  {
    code: "let flag = true and not false;",
    marks: [
      ["flag", "variable"],
      ["true", "boolean"],
      ["and", "keyword.operator"],
      ["not", "keyword.operator"],
    ],
  },
];

function columnOf(code, needle) {
  // A leading "." in the needle means "the character after the dot",
  // used to pick out a member name without also matching the dot itself.
  const skip = needle.startsWith(".") ? 1 : 0;
  const col = code.indexOf(needle);
  if (col === -1) throw new Error(`"${needle}" not found in: ${code}`);
  return col + skip;
}

const lines = ["// a leading comment", ""];
for (const { code, marks } of blocks) {
  lines.push(code);
  for (const [needle, capture] of marks) {
    const col = columnOf(code, needle);
    if (col < 2) throw new Error(`column ${col} for "${needle}" leaves no room for "//": ${code}`);
    lines.push("//" + " ".repeat(col - 2) + "^ " + capture);
  }
  lines.push("");
}
console.log(lines.join("\n"));
