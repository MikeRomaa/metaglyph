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

glyph C (codepoint: U+0043, advance: glyph.bbox.x1 + sidebear) {
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

glyph D (codepoint: U+0044, advance: glyph.bbox.x1 + sidebear) {
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

glyph three (codepoint: U+0033, advance: glyph.bbox.x1 + sidebear) {
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
        start (at: neck)
        arc   (center: (neck.x, top * 0.28),   to: (sidebear + w, top * 0.28), sweep: "cw")
        arc   (center: (sidebear, top * 0.28), to: (sidebear, top * 0.10),     sweep: "cw")
    }
}

glyph six (codepoint: U+0036, advance: glyph.bbox.x1 + sidebear) {
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
