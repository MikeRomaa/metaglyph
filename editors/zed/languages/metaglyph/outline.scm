; Top level: font, each param/metric/instance/group/glyph, and each
; top-level `let` (anchored to source_file so glyph-scope `let`s inside a
; body are excluded).
(declaration kind: "font" @name) @item

(declaration
  kind: ["param" "metric" "instance" "group" "glyph"]
  name: (identifier) @name) @item

(source_file (let_statement name: (identifier) @name) @item)

; Nested under a glyph: its named paths and anchors.
(declaration
  kind: "glyph"
  body: (body
    (declaration
      kind: ["path" "anchor"]
      name: (identifier) @name) @item))

; An unnamed path still gets an outline entry, labelled by its keyword.
(declaration
  kind: "glyph"
  body: (body
    (declaration kind: "path" @name !name) @item))

; `kern (left: A, right: V, by: -20)` -> "kern A V".
(declaration
  kind: "kern" @context
  config: (config
    (field value: (identifier) @context.extra)
    (field value: (identifier) @name))) @item
