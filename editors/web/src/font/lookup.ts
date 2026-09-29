import type {
    FontData,
    GlyphInfo,
    GlyphScene,
    KernInfo,
    PathInfo,
    Span,
} from "../engine/types.ts";
import type { Selection } from "../state/store.ts";

/** A number as the design prints it: at most one decimal place. */
export function fmt(n: number | undefined): string {
    if (n === undefined || Number.isNaN(n)) return "—";
    const s = n.toFixed(1).replace(/\.0$/, "");
    return s === "-0" ? "0" : s.replace("-", "−");
}

export function hex(cp: number): string {
    return cp.toString(16).toUpperCase().padStart(4, "0");
}

/** A metric's evaluated `y`, or `fallback`. */
export function metricY(
    font: FontData,
    name: string,
    fallback: number,
): number {
    return font.metrics.find((m) => m.name === name)?.y ?? fallback;
}

/** The vertical extent every drawing fits: descender to ascender. */
export function verticalExtent(font: FontData): [number, number] {
    return [
        metricY(font, "descender", -0.2 * font.em),
        metricY(font, "ascender", 0.8 * font.em),
    ];
}

export function spanContains(span: Span, offset: number): boolean {
    return span[0] <= offset && offset <= span[1];
}

/** Codepoint → glyph name. */
export function glyphsByCodepoint(font: FontData): Map<number, GlyphInfo> {
    const map = new Map<number, GlyphInfo>();
    for (const glyph of font.glyphs) {
        for (const cp of glyph.codepoints) {
            if (!map.has(cp)) map.set(cp, glyph);
        }
    }
    return map;
}

/** The glyphs a string sets, one per character that has a glyph. */
export function glyphsForText(font: FontData, text: string): GlyphInfo[] {
    const map = glyphsByCodepoint(font);
    const out: GlyphInfo[] = [];
    for (const ch of text) {
        const glyph = map.get(ch.codePointAt(0) ?? -1);
        if (glyph) out.push(glyph);
    }
    return out;
}

export interface Spacing {
    advance?: number;
    /** Placed left and right sidebearings (spec §12.1). */
    lsb?: number;
    rsb?: number;
}

export function spacing(glyph: GlyphInfo): Spacing {
    const { advance, ink } = glyph;
    const shift = glyph.shift ?? 0;
    if (!ink) return { advance };
    const lsb = ink[0] + shift;
    const rsb = advance === undefined ? undefined : advance - (ink[2] + shift);
    return { advance, lsb, rsb };
}

export type KernLevel = "glyph" | "group" | "mixed";

export function kernLevel(kern: KernInfo): KernLevel {
    if (kern.leftGroup && kern.rightGroup) return "group";
    if (kern.leftGroup || kern.rightGroup) return "mixed";
    return "glyph";
}

export interface EffectiveKern {
    value: number;
    /** Index into `font.kerns`. */
    index: number;
    level: KernLevel;
}

/** The kern applied between `left` and `right` (spec §12.2): a glyph pair
 * overrides a pair with a group on either side, which overrides a group
 * pair. */
export function effectiveKern(
    font: FontData,
    left: string,
    right: string,
): EffectiveKern | null {
    const inGroup = (group: string, glyph: string) =>
        font.groups.find((g) => g.name === group)?.glyphs.includes(glyph) ??
        false;
    const matches = (name: string | undefined, group: boolean, glyph: string) =>
        name !== undefined && (group ? inGroup(name, glyph) : name === glyph);

    let best: EffectiveKern | null = null;
    const rank = { glyph: 0, mixed: 1, group: 2 };
    font.kerns.forEach((kern, index) => {
        if (kern.by === undefined) return;
        if (!matches(kern.left, kern.leftGroup, left)) return;
        if (!matches(kern.right, kern.rightGroup, right)) return;
        const level = kernLevel(kern);
        if (!best || rank[level] < rank[best.level]) {
            best = { value: kern.by, index, level };
        }
    });
    return best;
}

/** A glyph name for one side of a kern: the glyph itself, or a group's
 * first member. */
export function sideGlyph(
    font: FontData,
    name: string | undefined,
    group: boolean,
): string | undefined {
    if (!group) return name;
    return font.groups.find((g) => g.name === name)?.glyphs[0];
}

/** A path's name, or `#<index>` for an anonymous one. */
export function pathKey(path: PathInfo): string {
    return path.name ?? `#${path.index}`;
}

/** `selection` in new data: the same kind and name, with its current span;
 * `null` if it no longer exists. */
export function locate(
    selection: Selection,
    font: FontData,
    scene: GlyphScene | null,
): Selection | null {
    const found = (span: Span | undefined) =>
        span ? { ...selection, span } : null;
    const { kind, name } = selection;
    switch (kind) {
        case "point":
            return found(scene?.points.find((p) => p.name === name)?.span);
        case "line":
            return found(
                scene?.lines.find((l) => !l.of && l.name === name)?.span,
            );
        case "path":
            return found(scene?.paths.find((p) => pathKey(p) === name)?.span);
        case "segment": {
            const [path, i] = name.split("/");
            const p = scene?.paths.find((x) => pathKey(x) === path);
            return found(p?.segments[Number(i)]?.span);
        }
        case "component":
            return found(scene?.components[Number(name)]?.span);
        case "glyph":
            return found(font.glyphs.find((g) => g.name === name)?.span);
        case "kern":
            return found(font.kerns[Number(name)]?.span);
        case "metric":
            return found(font.metrics.find((m) => m.name === name)?.span);
        case "let":
            return found(font.lets.find((l) => l.name === name)?.span);
        case "group":
            return found(font.groups.find((g) => g.name === name)?.span);
    }
}
