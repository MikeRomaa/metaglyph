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

glyph A (codepoint: U+0041, advance: glyph.bbox.x1 + sidebear) {
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

glyph B (codepoint: U+0042, advance: glyph.bbox.x1 + sidebear) {
  let w   = capW * 0.88;
  let sx  = ox;
  let top = capHeight.y;
  let mid = top * 0.53;

  path upright (stroke: stem) {
    start (at: (sx, top))
    line  (to: (sx, 0))
  }
  path bowlU (stroke: stem) {
    start  (at: (sx, top - stem/2),                   dir: right)
    spline (to: (sx + w - stem, (top - stem/2 + mid)/2), dir: down)
    spline (to: (sx, mid),                            dir: left)
  }
  path bowlL (stroke: stem) {
    start  (at: (sx, mid),                        dir: right)
    spline (to: (sx + w - stem/2, (mid + stem/2)/2), dir: down)
    spline (to: (sx, stem/2),                     dir: left)
  }
}

glyph C (codepoint: U+0043, advance: glyph.bbox.x1 + sidebear) {
  let w  = capW;
  let cy = capHeight.y / 2;

  path arc (
    stroke: stem,
    caps:  { start: "butt", end: "butt" },
  ) {
    start  (at: (ox + w * 0.93, capHeight.y * 0.79), dir: dir(152deg))
    spline (to: (ox + w/2, capHeight.ink - stem/2),  dir: left)
    spline (to: (ox, cy),                            dir: down)
    spline (to: (ox + w/2, baseline.ink + stem/2),   dir: right)
    spline (to: (ox + w * 0.93, capHeight.y * 0.21), dir: dir(28deg))
  }
}

glyph D (codepoint: U+0044, advance: glyph.bbox.x1 + sidebear) {
  let w   = capW * 0.94;
  let sx  = ox;
  let top = capHeight.y;

  path upright (stroke: stem) {
    start (at: (sx, top))
    line  (to: (sx, 0))
  }
  path bowl (stroke: stem) {
    start  (at: (sx, top - stem/2),       dir: right)
    spline (to: (sx + w - stem/2, top/2), dir: down)
    spline (to: (sx, stem/2),             dir: left)
  }
}

glyph E (codepoint: U+0045, advance: glyph.bbox.x1 + sidebear) {
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

glyph F (codepoint: U+0046, advance: glyph.bbox.x1 + sidebear) {
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

glyph zero (codepoint: U+0030, advance: glyph.bbox.x1 + sidebear) {
  let w  = figW * 0.86;
  let cy = figHeight.y / 2;

  path bowl (stroke: stem) {
    start  (at: (ox + w/2, figHeight.ink - stem/2), dir: right)
    spline (to: (ox + w, cy),                       dir: down)
    spline (to: (ox + w/2, baseline.ink + stem/2),  dir: left)
    spline (to: (ox, cy),                           dir: up)
    close
  }
}

glyph one (codepoint: U+0031, advance: glyph.bbox.x1 + sidebear) {
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

glyph two (codepoint: U+0032, advance: glyph.bbox.x1 + sidebear) {
  let w    = figW;
  let top  = figHeight.y;
  let turn = (ox + w * 0.93, top * 0.66);

  path arc (stroke: stem) {
    start  (at: (ox, top * 0.78),                   dir: up)
    spline (to: (ox + w/2, figHeight.ink - stem/2), dir: right)
    spline (to: turn,                               dir: down)
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

glyph three (codepoint: U+0033, advance: glyph.bbox.x1 + sidebear) {
  let w   = figW * 0.90;
  let top = figHeight.y;
  let mid = (ox + w * 0.44, top * 0.52);

  path arcU (stroke: stem) {
    start  (at: (ox, top * 0.80),                   dir: up)
    spline (to: (ox + w/2, figHeight.ink - stem/2), dir: right)
    spline (to: mid,                                dir: down)
  }
  path arcL (stroke: stem) {
    start  (at: mid,                   dir: right)
    spline (to: (ox + w, top * 0.26),  dir: down)
    spline (to: (ox, top * 0.14),      dir: left)
  }
}

glyph four (codepoint: U+0034, advance: glyph.bbox.x1 + sidebear) {
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

glyph five (codepoint: U+0035, advance: glyph.bbox.x1 + sidebear) {
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
    start  (at: neck,                       dir: right)
    spline (to: (sidebear + w, top * 0.28), dir: down)
    spline (to: (sidebear, top * 0.10),     dir: left)
  }
}

glyph six (codepoint: U+0036, advance: glyph.bbox.x1 + sidebear) {
  let w  = figW * 0.88;
  let cy = figHeight.y * 0.30;
  let by = cy * 2;

  path spine (stroke: stem) {
    start  (at: (ox + w * 0.88, figHeight.y * 0.86),     dir: dir(160deg))
    spline (to: (ox + w * 0.40, figHeight.ink - stem/2), dir: left)
    spline (to: (ox, cy),                                dir: down)
  }
  path bowl (stroke: stem) {
    start  (at: (ox, cy),                          dir: down)
    spline (to: (ox + w/2, baseline.ink + stem/2), dir: right)
    spline (to: (ox + w, cy),                      dir: up)
    spline (to: (ox + w/2, by),                    dir: left)
    close                                          // back to (ox, cy), tangent down
  }
}

glyph seven (codepoint: U+0037, advance: glyph.bbox.x1 + sidebear) {
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

glyph eight (codepoint: U+0038, advance: glyph.bbox.x1 + sidebear) {
  let w     = figW * 0.86;
  let waist = figHeight.y * 0.53;
  let uw    = w * 0.84;
  let ux    = ox + (w - uw) / 2;
  let uy    = (waist + figHeight.y) / 2;

  path bowlU (stroke: stem) {
    start  (at: (ox + w/2, figHeight.ink - stem/2), dir: right)
    spline (to: (ux + uw, uy),                      dir: down)
    spline (to: (ox + w/2, waist),                  dir: left)
    spline (to: (ux, uy),                           dir: up)
    close
  }
  path bowlL (stroke: stem) {
    start  (at: (ox + w/2, waist),                 dir: right)
    spline (to: (ox + w, waist/2),                 dir: down)
    spline (to: (ox + w/2, baseline.ink + stem/2), dir: left)
    spline (to: (ox, waist/2),                     dir: up)
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
