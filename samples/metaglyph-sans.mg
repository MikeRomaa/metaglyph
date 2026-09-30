// ══ Metaglyph Sans ═══════════════════════════════════════════════════
// Sample source: A–Z, 0–9. Monospaced: every glyph fills one `cell`.
// Strokes meet at right angles; rounds are straight sides joined by
// quarter-circle corners of radius `rad`. Diagonals are filled bands, so
// they end flat on the baseline and cap height.

font (name: "Metaglyph Sans", em: 1000)

// ── design space ─────────────────────────────────────────────────────
param stem   (default: 90, range: 40..160)
param corner (default: 50, range: 10..100)  // corner radius past stem / 2
param barPos (default: 0.48, range: 0.40..0.56)  // crossbar height ratio

let cell = 600;  // every glyph's advance
let side = 60;   // sidebearing to the widest ink
let x0 = side;         // left ink edge
let x1 = cell - side;  // right ink edge
let xl = x0 + stem / 2;  // left stem centreline
let xr = x1 - stem / 2;  // right stem centreline
let xm = cell / 2;
let yt = capHeight.y - stem / 2;  // top bar centreline
let yb = stem / 2;                // bottom bar centreline
let ym = capHeight.y * barPos;    // crossbar centreline
let rad = stem / 2 + corner;      // corner radius on the centreline

// ── vertical metrics ─────────────────────────────────────────────────
metric baseline  (y: 0, align: "bottom")
metric xHeight   (y: 520)
metric capHeight (y: 700)
metric ascender  (y: 760)
metric descender (y: -220, align: "bottom")

// ── instances ────────────────────────────────────────────────────────
instance Light   (stem: 50, weightClass: 300)
instance Regular ()
instance Bold    (stem: 140, weightClass: 700)

// ══ Capitals ═════════════════════════════════════════════════════════

glyph A (codepoint: 'A', advance: cell) {
    let dw = stem * length((xm, capHeight.y) - (xl, 0)) / capHeight.y;  // a leg's width across
    let half = (dw / 2, 0);
    let footL = (x0 + dw / 2, 0);
    let apexL = (xm - dw * 0.15, capHeight.y);
    let footR = (x1 - dw / 2, 0);
    let apexR = (xm + dw * 0.15, capHeight.y);
    let bar_y = hline(ym * 0.62);

    path legL (fill: true) {
        start (at: footL - half)
        line  (to: footL + half)
        line  (to: apexL + half)
        line  (to: apexL - half)
        close
    }
    path legR (fill: true) {
        start (at: footR - half)
        line  (to: footR + half)
        line  (to: apexR + half)
        line  (to: apexR - half)
        close
    }
    path bar (stroke: stem) {
        start (at: meet(lineThrough(footL, apexL), bar_y))
        line  (to: meet(lineThrough(footR, apexR), bar_y))
    }
}

glyph B (codepoint: 'B', advance: cell) {
    let ux = xr - 24;  // the upper bowl is a little narrower

    path upright (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path bowlU (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (ux - rad, yt))
        arc   (center: (ux - rad, yt - rad), to: (ux, yt - rad), sweep: "cw")
        line  (to: (ux, ym + rad))
        arc   (center: (ux - rad, ym + rad), to: (ux - rad, ym), sweep: "cw")
        line  (to: (x0, ym))
    }
    path bowlL (stroke: stem) {
        start (at: (x0, ym))
        line  (to: (xr - rad, ym))
        arc   (center: (xr - rad, ym - rad), to: (xr, ym - rad), sweep: "cw")
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
}

glyph C (codepoint: 'C', advance: cell) {
    path bowl (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl + rad, yt))
        arc   (center: (xl + rad, yt - rad), to: (xl, yt - rad), sweep: "ccw")
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (x1, yb))
    }
}

glyph D (codepoint: 'D', advance: cell) {
    path upright (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path bowl (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (xr - rad, yt))
        arc   (center: (xr - rad, yt - rad), to: (xr, yt - rad), sweep: "cw")
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
}

glyph E (codepoint: 'E', advance: cell) {
    path frame (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl, yt))
        line  (to: (xl, yb))
        line  (to: (x1, yb))
    }
    path bar (stroke: stem) {
        start (at: (xl, ym))
        line  (to: (x1 - 50, ym))
    }
}

glyph F (codepoint: 'F', advance: cell) {
    path frame (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl, yt))
        line  (to: (xl, 0))
    }
    path bar (stroke: stem) {
        start (at: (xl, ym))
        line  (to: (x1 - 50, ym))
    }
}

glyph G (codepoint: 'G', advance: cell) {
    path bowl (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl + rad, yt))
        arc   (center: (xl + rad, yt - rad), to: (xl, yt - rad), sweep: "ccw")
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (xr - rad, yb))
        arc   (center: (xr - rad, yb + rad), to: (xr, yb + rad), sweep: "ccw")
        line  (to: (xr, ym))
        line  (to: (xm, ym))
    }
}

glyph H (codepoint: 'H', advance: cell) {
    path stemL (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path stemR (stroke: stem) {
        start (at: (xr, 0))
        line  (to: (xr, capHeight.y))
    }
    path bar (stroke: stem) {
        start (at: (xl, ym))
        line  (to: (xr, ym))
    }
}

glyph I (codepoint: 'I', advance: cell) {
    let serif = 0.30 * cell;  // half the width of the top and bottom bars

    path upright (stroke: stem) {
        start (at: (xm, 0))
        line  (to: (xm, capHeight.y))
    }
    path barT (stroke: stem) {
        start (at: (xm - serif, yt))
        line  (to: (xm + serif, yt))
    }
    path barB (stroke: stem) {
        start (at: (xm - serif, yb))
        line  (to: (xm + serif, yb))
    }
}

glyph J (codepoint: 'J', advance: cell) {
    path hook (stroke: stem) {
        start (at: (xr, capHeight.y))
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
    path bar (stroke: stem) {
        start (at: (xm - 60, yt))
        line  (to: (x1, yt))
    }
}

glyph K (codepoint: 'K', advance: cell) {
    let dw = stem * length((xr, capHeight.y) - (xl, ym)) / (capHeight.y - ym);
    let half = (dw / 2, 0);
    let joint = (x0 + dw / 2, ym);
    let armEnd = (x1 - dw / 2, capHeight.y);
    let legEnd = (x1 - dw / 2, 0);

    path upright (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path arm (fill: true) {
        start (at: joint - half)
        line  (to: joint + half)
        line  (to: armEnd + half)
        line  (to: armEnd - half)
        close
    }
    path leg (fill: true) {
        start (at: legEnd - half)
        line  (to: legEnd + half)
        line  (to: joint + half)
        line  (to: joint - half)
        close
    }
}

glyph L (codepoint: 'L', advance: cell) {
    path frame (stroke: stem) {
        start (at: (xl, capHeight.y))
        line  (to: (xl, yb))
        line  (to: (x1, yb))
    }
}

glyph M (codepoint: 'M', advance: cell) {
    let vy = capHeight.y * 0.30;  // the flat bottom of the middle V
    let dw = stem * length((xl, capHeight.y) - (xm, vy)) / (capHeight.y - vy);
    let half = (dw / 2, 0);
    let topL = (x0 + dw / 2, capHeight.y);
    let topR = (x1 - dw / 2, capHeight.y);
    let vL = (xm - dw * 0.15, vy);
    let vR = (xm + dw * 0.15, vy);

    path stemL (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path stemR (stroke: stem) {
        start (at: (xr, 0))
        line  (to: (xr, capHeight.y))
    }
    path diagL (fill: true) {
        start (at: vL - half)
        line  (to: vL + half)
        line  (to: topL + half)
        line  (to: topL - half)
        close
    }
    path diagR (fill: true) {
        start (at: vR - half)
        line  (to: vR + half)
        line  (to: topR + half)
        line  (to: topR - half)
        close
    }
}

glyph N (codepoint: 'N', advance: cell) {
    let dw = stem * length((xr, 0) - (xl, capHeight.y)) / capHeight.y;
    let half = (dw / 2, 0);
    let top = (x0 + dw / 2, capHeight.y);
    let foot = (x1 - dw / 2, 0);

    path stemL (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path stemR (stroke: stem) {
        start (at: (xr, 0))
        line  (to: (xr, capHeight.y))
    }
    path diag (fill: true) {
        start (at: foot - half)
        line  (to: foot + half)
        line  (to: top + half)
        line  (to: top - half)
        close
    }
}

glyph O (codepoint: 'O', advance: cell) {
    path bowl (stroke: stem) {
        start (at: (xl, yt - rad))
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (xr - rad, yb))
        arc   (center: (xr - rad, yb + rad), to: (xr, yb + rad), sweep: "ccw")
        line  (to: (xr, yt - rad))
        arc   (center: (xr - rad, yt - rad), to: (xr - rad, yt), sweep: "ccw")
        line  (to: (xl + rad, yt))
        arc   (center: (xl + rad, yt - rad), to: (xl, yt - rad), sweep: "ccw")
        close
    }
}

glyph P (codepoint: 'P', advance: cell) {
    path upright (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path bowl (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (xr - rad, yt))
        arc   (center: (xr - rad, yt - rad), to: (xr, yt - rad), sweep: "cw")
        line  (to: (xr, ym + rad))
        arc   (center: (xr - rad, ym + rad), to: (xr - rad, ym), sweep: "cw")
        line  (to: (x0, ym))
    }
}

glyph Q (codepoint: 'Q', advance: cell) {
    component (glyph: O)
    path tail (stroke: stem) {
        start (at: (xm, yb + 140))
        cube  (c1: (xm + 30, yb + 40), c2: (xm + 80, -90), to: (x1, -90))
    }
}

glyph R (codepoint: 'R', advance: cell) {
    let dw = stem * length((xr, 0) - (xm, ym)) / ym;
    let half = (dw / 2, 0);
    let knee = (xm, ym);
    let foot = (x1 - dw / 2, 0);

    component (glyph: P)
    path leg (fill: true) {
        start (at: foot - half)
        line  (to: foot + half)
        line  (to: knee + half)
        line  (to: knee - half)
        close
    }
}

glyph S (codepoint: 'S', advance: cell) {
    path spine (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl + rad, yt))
        arc   (center: (xl + rad, yt - rad), to: (xl, yt - rad), sweep: "ccw")
        line  (to: (xl, ym + rad))
        arc   (center: (xl + rad, ym + rad), to: (xl + rad, ym), sweep: "ccw")
        line  (to: (xr - rad, ym))
        arc   (center: (xr - rad, ym - rad), to: (xr, ym - rad), sweep: "cw")
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
}

glyph T (codepoint: 'T', advance: cell) {
    path bar (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (x1, yt))
    }
    path upright (stroke: stem) {
        start (at: (xm, 0))
        line  (to: (xm, yt))
    }
}

glyph U (codepoint: 'U', advance: cell) {
    path bowl (stroke: stem) {
        start (at: (xl, capHeight.y))
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (xr - rad, yb))
        arc   (center: (xr - rad, yb + rad), to: (xr, yb + rad), sweep: "ccw")
        line  (to: (xr, capHeight.y))
    }
}

glyph V (codepoint: 'V', advance: cell) {
    let dw = stem * length((xl, capHeight.y) - (xm, 0)) / capHeight.y;
    let half = (dw / 2, 0);
    let topL = (x0 + dw / 2, capHeight.y);
    let topR = (x1 - dw / 2, capHeight.y);
    let footL = (xm - dw * 0.15, 0);
    let footR = (xm + dw * 0.15, 0);

    path armL (fill: true) {
        start (at: footL - half)
        line  (to: footL + half)
        line  (to: topL + half)
        line  (to: topL - half)
        close
    }
    path armR (fill: true) {
        start (at: footR - half)
        line  (to: footR + half)
        line  (to: topR + half)
        line  (to: topR - half)
        close
    }
}

glyph W (codepoint: 'W', advance: cell) {
    let wy = capHeight.y * 0.62;  // the flat top of the middle peak
    let dw = stem * length((xl, 0) - (xm, wy)) / wy;
    let half = (dw / 2, 0);
    let footL = (x0 + dw / 2, 0);
    let footR = (x1 - dw / 2, 0);
    let peakL = (xm - dw * 0.15, wy);
    let peakR = (xm + dw * 0.15, wy);

    path stemL (stroke: stem) {
        start (at: (xl, 0))
        line  (to: (xl, capHeight.y))
    }
    path stemR (stroke: stem) {
        start (at: (xr, 0))
        line  (to: (xr, capHeight.y))
    }
    path diagL (fill: true) {
        start (at: footL - half)
        line  (to: footL + half)
        line  (to: peakL + half)
        line  (to: peakL - half)
        close
    }
    path diagR (fill: true) {
        start (at: footR - half)
        line  (to: footR + half)
        line  (to: peakR + half)
        line  (to: peakR - half)
        close
    }
}

glyph X (codepoint: 'X', advance: cell) {
    let dw = stem * length((xr, capHeight.y) - (xl, 0)) / capHeight.y;
    let half = (dw / 2, 0);
    let lo = x0 + dw / 2;
    let hi = x1 - dw / 2;

    path rising (fill: true) {
        start (at: (lo, 0) - half)
        line  (to: (lo, 0) + half)
        line  (to: (hi, capHeight.y) + half)
        line  (to: (hi, capHeight.y) - half)
        close
    }
    path falling (fill: true) {
        start (at: (hi, 0) - half)
        line  (to: (hi, 0) + half)
        line  (to: (lo, capHeight.y) + half)
        line  (to: (lo, capHeight.y) - half)
        close
    }
}

glyph Y (codepoint: 'Y', advance: cell) {
    let dw = stem * length((xl, capHeight.y) - (xm, ym)) / (capHeight.y - ym);
    let half = (dw / 2, 0);
    let topL = (x0 + dw / 2, capHeight.y);
    let topR = (x1 - dw / 2, capHeight.y);
    // The arms' outer edges land on the stem's edges.
    let forkL = (xm - stem / 2 + dw / 2, ym);
    let forkR = (xm + stem / 2 - dw / 2, ym);

    path upright (stroke: stem) {
        start (at: (xm, 0))
        line  (to: (xm, ym))
    }
    path armL (fill: true) {
        start (at: forkL - half)
        line  (to: forkL + half)
        line  (to: topL + half)
        line  (to: topL - half)
        close
    }
    path armR (fill: true) {
        start (at: forkR - half)
        line  (to: forkR + half)
        line  (to: topR + half)
        line  (to: topR - half)
        close
    }
}

glyph Z (codepoint: 'Z', advance: cell) {
    let dw = stem * length((xr, yt) - (xl, yb)) / (yt - yb);
    let half = (dw / 2, 0);
    let low = (x0 + dw / 2, yb);
    let high = (x1 - dw / 2, yt);

    path barT (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (x1, yt))
    }
    path barB (stroke: stem) {
        start (at: (x0, yb))
        line  (to: (x1, yb))
    }
    path diag (fill: true) {
        start (at: low - half)
        line  (to: low + half)
        line  (to: high + half)
        line  (to: high - half)
        close
    }
}

// ══ Figures ══════════════════════════════════════════════════════════

glyph zero (codepoint: '0', advance: cell) {
    let zl = xl + 30;
    let zr = xr - 30;
    let cBL = (zl + rad, yb + rad);  // corner centres
    let cTR = (zr - rad, yt - rad);
    let slope = angle(cTR - cBL);
    // The slash ends on the corners where they face along it.
    let slash0 = polar(cBL, rad, slope + 180deg);
    let slash1 = polar(cTR, rad, slope);

    path bowl (stroke: stem) {
        start (at: (zl, yt - rad))
        line  (to: (zl, yb + rad))
        arc   (center: cBL, to: (zl + rad, yb), sweep: "ccw")
        line  (to: (zr - rad, yb))
        arc   (center: (zr - rad, yb + rad), to: (zr, yb + rad), sweep: "ccw")
        line  (to: (zr, yt - rad))
        arc   (center: cTR, to: (zr - rad, yt), sweep: "ccw")
        line  (to: (zl + rad, yt))
        arc   (center: (zl + rad, yt - rad), to: (zl, yt - rad), sweep: "ccw")
        close
    }
    path slash (stroke: stem * 0.8) {
        start (at: slash0)
        line  (to: slash1)
    }
}

glyph one (codepoint: '1', advance: cell) {
    let serif = 0.30 * cell;

    path upright (stroke: stem) {
        start (at: (xm, 0))
        line  (to: (xm, capHeight.y))
    }
    path flag (stroke: stem) {
        start (at: (xm, yt))
        line  (to: (xm - serif * 0.8, yt - 110))
    }
    path base (stroke: stem) {
        start (at: (xm - serif, yb))
        line  (to: (xm + serif, yb))
    }
}

glyph two (codepoint: '2', advance: cell) {
    path spine (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (xr - rad, yt))
        arc   (center: (xr - rad, yt - rad), to: (xr, yt - rad), sweep: "cw")
        line  (to: (xr, ym + rad))
        arc   (center: (xr - rad, ym + rad), to: (xr - rad, ym), sweep: "cw")
        line  (to: (xl, ym))
        line  (to: (xl, yb))
        line  (to: (x1, yb))
    }
}

glyph three (codepoint: '3', advance: cell) {
    path bowl (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (xr - rad, yt))
        arc   (center: (xr - rad, yt - rad), to: (xr, yt - rad), sweep: "cw")
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
    path bar (stroke: stem) {
        start (at: (xm - 40, ym))
        line  (to: (xr, ym))
    }
}

glyph four (codepoint: '4', advance: cell) {
    let sx = x1 - stem * 1.5;  // the upright's centreline
    let barY = ym * 0.8;

    path frame (stroke: stem) {
        start (at: (xl, capHeight.y))
        line  (to: (xl, barY))
        line  (to: (x1, barY))
    }
    path upright (stroke: stem) {
        start (at: (sx, 0))
        line  (to: (sx, capHeight.y))
    }
}

glyph five (codepoint: '5', advance: cell) {
    let neck = ym + 20;

    path spine (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl, yt))
        line  (to: (xl, neck))
        line  (to: (xr - rad, neck))
        arc   (center: (xr - rad, neck - rad), to: (xr, neck - rad), sweep: "cw")
        line  (to: (xr, yb + rad))
        arc   (center: (xr - rad, yb + rad), to: (xr - rad, yb), sweep: "cw")
        line  (to: (x0, yb))
    }
}

glyph six (codepoint: '6', advance: cell) {
    path spine (stroke: stem) {
        start (at: (x1, yt))
        line  (to: (xl + rad, yt))
        arc   (center: (xl + rad, yt - rad), to: (xl, yt - rad), sweep: "ccw")
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (xr - rad, yb))
        arc   (center: (xr - rad, yb + rad), to: (xr, yb + rad), sweep: "ccw")
        line  (to: (xr, ym - rad))
        arc   (center: (xr - rad, ym - rad), to: (xr - rad, ym), sweep: "ccw")
        line  (to: (xl, ym))
    }
}

glyph seven (codepoint: '7', advance: cell) {
    let dw = stem * length((xr, yt) - (xm, 0)) / yt;
    let half = (dw / 2, 0);
    let top = (x1 - dw / 2, yt);
    let foot = (xm - stem * 0.3, 0);

    path bar (stroke: stem) {
        start (at: (x0, yt))
        line  (to: (x1, yt))
    }
    path diag (fill: true) {
        start (at: foot - half)
        line  (to: foot + half)
        line  (to: top + half)
        line  (to: top - half)
        close
    }
}

glyph eight (codepoint: '8', advance: cell) {
    let ul = xl + 20;  // the upper bowl is a little narrower
    let ur = xr - 20;

    path bowlU (stroke: stem) {
        start (at: (ul, yt - rad))
        line  (to: (ul, ym + rad))
        arc   (center: (ul + rad, ym + rad), to: (ul + rad, ym), sweep: "ccw")
        line  (to: (ur - rad, ym))
        arc   (center: (ur - rad, ym + rad), to: (ur, ym + rad), sweep: "ccw")
        line  (to: (ur, yt - rad))
        arc   (center: (ur - rad, yt - rad), to: (ur - rad, yt), sweep: "ccw")
        line  (to: (ul + rad, yt))
        arc   (center: (ul + rad, yt - rad), to: (ul, yt - rad), sweep: "ccw")
        close
    }
    path bowlL (stroke: stem) {
        start (at: (xl, ym - rad))
        line  (to: (xl, yb + rad))
        arc   (center: (xl + rad, yb + rad), to: (xl + rad, yb), sweep: "ccw")
        line  (to: (xr - rad, yb))
        arc   (center: (xr - rad, yb + rad), to: (xr, yb + rad), sweep: "ccw")
        line  (to: (xr, ym - rad))
        arc   (center: (xr - rad, ym - rad), to: (xr - rad, ym), sweep: "ccw")
        line  (to: (xl + rad, ym))
        arc   (center: (xl + rad, ym - rad), to: (xl, ym - rad), sweep: "ccw")
        close
    }
}

glyph nine (codepoint: '9', advance: cell) {
    // 6 turned half a turn about the cell's centre.
    component (glyph: six,
               transform: (rotate(180deg), translate(cell, capHeight.y)))
}
