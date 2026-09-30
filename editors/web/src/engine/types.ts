// Mirrors the serde types in crates/mg-web/src/{doc,view}.rs.

export type Severity = "error" | "warning";

export interface DiagnosticInfo {
    /** UTF-16 offsets into the checked text. */
    from: number;
    to: number;
    severity: Severity;
    code: string;
    message: string;
}

export interface FontInfo {
    name?: string;
    version: string;
    designer?: string;
    foundry?: string;
    em?: number;
}

export interface DocState {
    version: number;
    /** False when the text has syntax errors; the canvas goes read-only. */
    parseOk: boolean;
    /** False when errors stop evaluation (syntax, an unresolved name, a
     * mistyped call): the views keep the last text that evaluated. */
    evaluated: boolean;
    diagnostics: DiagnosticInfo[];
    font?: FontInfo;
    instances: string[];
    glyphCount: number;
}

/** A UTF-16 `[from, to]` source range. */
export type Span = [number, number];
export type Pt = [number, number];

export interface MetricInfo {
    name: string;
    y?: number;
    overshoot?: number;
    expr: string;
    span: Span;
}

export interface LetInfo {
    name: string;
    expr: string;
    value: string;
    span: Span;
}

export interface GlyphInfo {
    name: string;
    codepoints: number[];
    span: Span;
    advance?: number;
    /** Authored → placed x offset (spec §12.1). */
    shift?: number;
    /** Authored ink bounds `[x0, y0, x1, y1]`; absent with no ink. */
    ink?: [number, number, number, number];
    fields: { advance?: string; lsb?: string; rsb?: string };
    /** Decomposed outline in placed coordinates, one SVG `d`. */
    outline: string;
    components: number;
    errors: number;
}

export interface GroupInfo {
    name: string;
    glyphs: string[];
    span: Span;
}

export interface KernInfo {
    left?: string;
    leftGroup: boolean;
    right?: string;
    rightGroup: boolean;
    by?: number;
    expr: string;
    span: Span;
}

export interface FontData {
    instance: string;
    em: number;
    metrics: MetricInfo[];
    lets: LetInfo[];
    glyphs: GlyphInfo[];
    groups: GroupInfo[];
    kerns: KernInfo[];
}

export interface ComponentInfo {
    glyph: string;
    outline: string;
    span: Span;
}

export type SegmentKind = "start" | "line" | "quad" | "cube" | "arc";

export interface SegmentInfo {
    kind: SegmentKind;
    name?: string;
    span: Span;
    to?: Pt;
    toRef?: string;
    controls: Pt[];
    /** An `arc`'s ellipse; absent for other kinds or when it failed. */
    arc?: ArcInfo;
}

export interface ArcInfo {
    center: Pt;
    rx: number;
    ry: number;
    /** Where the arc starts: the previous segment's end. */
    from: Pt;
    /** Radii mode's `rx`/`ry` source text; absent in centre mode. */
    rxExpr?: string;
    ryExpr?: string;
    sweep: "ccw" | "cw";
    /** Radii mode's `large`; absent in centre mode. */
    large?: boolean;
}

export interface PathInfo {
    index: number;
    name?: string;
    span: Span;
    stroke?: string;
    caps?: [string, string];
    joins: string;
    fill: boolean;
    closed: boolean;
    skeleton?: string;
    /** A point on the skeleton for the path's callout to touch: halfway
     * along its first segment. */
    anchor?: Pt;
    segments: SegmentInfo[];
}

export interface PointInfo {
    name: string;
    at: Pt;
    role: "skeleton" | "construction";
    expr: string;
    callee?: string;
    span: Span;
}

export interface LineInfo {
    name: string;
    p0: Pt;
    p1: Pt;
    expr: string;
    span: Span;
    /** For the ray a `polar(q, len, θ)` point is placed along: that
     * point's name. The line runs from `q` through the point. */
    of?: string;
    /** For a `polar` ray: the `len` argument's source text. */
    radiusExpr?: string;
}

export interface GlyphScene {
    name: string;
    span: Span;
    /** Own contours, authored coordinates. */
    outline: string[];
    components: ComponentInfo[];
    paths: PathInfo[];
    points: PointInfo[];
    lines: LineInfo[];
    /** Measurements, `let dN = length(b - a);`. */
    measures: MeasureInfo[];
}

export interface MeasureInfo {
    name: string;
    a: Pt;
    b: Pt;
    value: number;
    span: Span;
}

/** What the views render: one instance's data and the active glyph. */
export interface View {
    instance: string | null;
    font: FontData | null;
    /** The active glyph, resolved: the requested one if it exists, else the
     * first glyph. */
    glyph: string | null;
    scene: GlyphScene | null;
}

/** An edit op (crates/mg-web/src/ops.rs). Declarations are addressed by
 * their source span in the version the op is sent with. */
export type Op =
    | { op: "rename"; span: Span; name: string }
    | { op: "delete"; span: Span }
    | { op: "addPoint"; glyph: string; at: Pt }
    | { op: "addLine"; glyph: string; line: LineSpec }
    | { op: "addMeasure"; glyph: string; a: string; b: string }
    | { op: "pathStart"; glyph: string; at: Pt; copyFrom?: string }
    | {
          op: "pathAppend";
          glyph: string;
          path: string;
          at: Pt;
          c1?: Pt;
          c2?: Pt;
      }
    | { op: "pathClose"; glyph: string; path: string }
    | { op: "setSegmentKind"; span: Span; kind: SegmentKind; controls: Pt[] }
    | { op: "setField"; span: Span; name: string; value: FieldValue }
    | { op: "removeField"; span: Span; name: string }
    | { op: "setFill"; span: Span; on: boolean }
    | { op: "addComponent"; glyph: string; target: string; offset: Pt }
    | { op: "relate"; span: Span; relation: Relation };

export type LineSpec =
    | { kind: "through"; a: string; b: string }
    | { kind: "hline"; y: number }
    | { kind: "vline"; x: number };

/** `expr` is text the user typed, spliced as-is. */
export type FieldValue =
    | { type: "str"; value: string }
    | { type: "num"; value: number }
    | { type: "bool"; value: boolean }
    | { type: "expr"; value: string };

/** One replacement, in UTF-16 offsets of the text its step applies to. */
export interface Change {
    from: number;
    to: number;
    insert: string;
}

/** A declaration an op created, to select and offer to rename. */
export interface Created {
    kind: "point" | "line" | "path" | "let" | "component";
    name: string;
}

export type EditResult =
    | {
          status: "ok";
          version: number;
          /** `steps[i]` applies to the text after `steps[..i]`. */
          steps: Change[][];
          created?: Created;
      }
    | { status: "stale" }
    | { status: "readOnly" }
    | { status: "invalid"; message: string };

/** One literal a point drag can rewrite (crates/mg-web/src/drag.rs). */
export interface DriverInfo {
    literal: string;
    /** The `let` whose expression holds it. */
    owner: string;
    value: number;
    /** How far the point moves per unit of the literal, per axis. */
    sens: Pt;
    linear: boolean;
}

export interface DragInfo {
    target: string;
    at: Pt;
    drivers: DriverInfo[];
    /** The driver for x and for y; null: that axis is locked. */
    axis: [number | null, number | null];
    /** One driver moves both axes: the point follows its track. */
    track: boolean;
    trackPoints: Pt[];
    /** For a locked axis: the top-level names it depends on. */
    lockedBy: [string[], string[]];
    /** Other points the target is placed from: a drag never moves them
     * (they are dragged directly), so they lock what they set. */
    anchors: string[];
}

/** What locks an axis, for messages: `stem1, stem2 (points) · h
 * (top-level)`. */
export function lockText(info: DragInfo, axis: 0 | 1): string {
    const parts = [
        info.anchors.length ? `${info.anchors.join(", ")} (points)` : "",
        info.lockedBy[axis].length
            ? `${info.lockedBy[axis].join(", ")} (top-level)`
            : "",
    ].filter(Boolean);
    return parts.join(" · ") || "no local literal";
}

export interface DragStep {
    /** Changes to the drag-start text. */
    changes: Change[];
    at: Pt;
    /** Each moved driver's new literal text. */
    literals: [number, string][];
    exact: boolean;
    /** A driver stopped at its extreme: further would make the source
     * invalid. */
    limited: boolean;
}

/** A relationship tool's rewrite (plan 5, §1.4). */
export type Relation =
    | { kind: "coincident"; b: string }
    | { kind: "meet"; l1: LineRef; l2: LineRef }
    | { kind: "project"; at: Pt; line: LineRef }
    | { kind: "fraction"; a: string; b: string; t: number }
    | { kind: "polar"; q: string; len: number; angle: number }
    | { kind: "mirror"; q: string; axis: LineRef };

/** A named line, or `lineThrough` two named points. */
export type LineRef = { name: string } | { through: [string, string] };

export interface EngineResult {
    doc?: DocState;
    view: View;
}
