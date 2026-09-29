import type { GlyphInfo } from "../engine/types.ts";

/** A glyph's placed outline, fitted between `descender` and `ascender`. */
export function GlyphThumb({
    glyph,
    ascender,
    descender,
    className,
}: {
    glyph: GlyphInfo;
    ascender: number;
    descender: number;
    className?: string;
}) {
    const advance = glyph.advance ?? ascender - descender;
    const pad = (ascender - descender) * 0.06;
    const viewBox = [
        -pad,
        -ascender - pad,
        advance + 2 * pad,
        ascender - descender + 2 * pad,
    ].join(" ");
    return (
        <svg className={className} viewBox={viewBox} aria-hidden>
            <path
                d={glyph.outline}
                transform="scale(1 -1)"
                style={{ fill: "var(--ink)" }}
            />
        </svg>
    );
}
