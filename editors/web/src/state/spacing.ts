// Spacing sheet edits (plan 5, §1.6): typed spacing fields, metric lines
// and font info. Numbers typed into a numeric field add-constant (which
// replaces a bare literal); anything else is spliced as an expression.

import type { Edge, GlyphInfo, MetricInfo, Op } from "../engine/types.ts";
import { spacing } from "../font/lookup.ts";
import { performEdit } from "./actions.ts";
import { useStore } from "./store.ts";

export type SpacingField = "advance" | "lsb" | "rsb";

/** A typed plain number (`−` accepted for minus), or null. */
export function parseNumber(text: string): number | null {
    const t = text.trim().replace(/^[−–]/, "-");
    return /^-?(\d+\.?\d*|\.\d+)$/.test(t) ? Number(t) : null;
}

function refuse(message: string): null {
    useStore.getState().setNotice(message, "info");
    return null;
}

/** The edge whose bearing a field's change moves: `advance` and `rsb`
 * both move the advance guide. */
export function edgeOf(field: SpacingField): Edge {
    return field === "lsb" ? "left" : "right";
}

/** A spacing guide op for glyph `g`: the bearing on `edge` changes by
 * `delta` (plan 5, §1.6 table). */
export function spacingOp(g: GlyphInfo, edge: Edge, delta: number): Op {
    const sp = spacing(g);
    return {
        op: "spacing",
        glyph: g.name,
        edge,
        delta,
        lsb: sp.lsb ?? 0,
        rsb: sp.rsb ?? 0,
        em: useStore.getState().font?.em ?? 1000,
    };
}

/**
 * The op for `text` typed into spacing field `field` of glyph `name`:
 * - empty removes a declared field, if another one remains;
 * - a number add-constants a declared field, or, for a derived one,
 *   moves the guide it measures to (which may declare a field);
 * - anything else is spliced into a declared field.
 */
export function typedSpacingOp(
    name: string,
    field: SpacingField,
    text: string,
): Op | null {
    const font = useStore.getState().font;
    const g = font?.glyphs.find((x) => x.name === name);
    if (!font || !g) return null;
    const declared = Object.entries(g.fields)
        .filter(([, v]) => v !== undefined)
        .map(([k]) => k);
    const own = declared.includes(field);
    const current = spacing(g)[field];
    const n = parseNumber(text);

    if (text === "") {
        if (!own) return null;
        if (declared.length === 1) {
            return refuse(
                `glyph ${name} must declare one or two of advance, lsb, rsb.`,
            );
        }
        return { op: "removeField", span: g.span, name: field };
    }
    if (own) {
        if (n !== null && current !== undefined) {
            return {
                op: "addConstant",
                span: g.span,
                name: field,
                delta: n - current,
                em: font.em,
            };
        }
        return {
            op: "setField",
            span: g.span,
            name: field,
            value: { type: "expr", value: text },
        };
    }
    if (n === null) {
        return refuse(
            `${field} is derived: type a number, and a declared field takes the change.`,
        );
    }
    if (current === undefined) {
        return refuse(`glyph ${name} has no ink to measure ${field} from.`);
    }
    return spacingOp(g, edgeOf(field), n - current);
}

export function typeSpacing(name: string, field: SpacingField, text: string) {
    return performEdit(`set_${field}`, () => typedSpacingOp(name, field, text));
}

/** `text` typed into metric `name`'s `y` or `overshoot`. */
export function typeMetric(
    name: string,
    field: "y" | "overshoot",
    text: string,
) {
    return performEdit(`set_${field}`, () => {
        const font = useStore.getState().font;
        const m: MetricInfo | undefined = font?.metrics.find(
            (x) => x.name === name,
        );
        if (!font || !m) return null;
        const source = field === "y" ? m.expr || undefined : m.overshootExpr;
        const current = field === "y" ? m.y : m.overshoot;
        const n = parseNumber(text);
        if (text === "") {
            if (field === "y") return refuse("A metric's y is required.");
            return source === undefined
                ? null
                : { op: "removeField", span: m.span, name: field };
        }
        if (source !== undefined && n !== null && current !== undefined) {
            return {
                op: "addConstant",
                span: m.span,
                name: field,
                delta: n - current,
                em: font.em,
            };
        }
        return {
            op: "setField",
            span: m.span,
            name: field,
            value: { type: "expr", value: text },
        };
    });
}

/** The `font (…)` fields the form edits; `name` and `em` are required. */
export const FONT_FIELDS = [
    { key: "name", label: "Name", required: true },
    { key: "version", label: "Version", required: false },
    { key: "designer", label: "Designer", required: false },
    { key: "foundry", label: "Foundry", required: false },
    { key: "license", label: "License", required: false },
    { key: "em", label: "Em", required: true },
] as const;

export type FontField = (typeof FONT_FIELDS)[number]["key"];

/** `text` typed into font field `name`: a string, or for `em` an integer;
 * empty removes an optional field. */
export function setFontField(name: FontField, text: string) {
    return performEdit(`set_font_${name}`, () => {
        const required = FONT_FIELDS.find((f) => f.key === name)?.required;
        if (text === "") {
            return required
                ? refuse(`font ${name} is required.`)
                : { op: "fontField", name };
        }
        if (name !== "em") {
            return {
                op: "fontField",
                name,
                value: { type: "str", value: text },
            };
        }
        const n = parseNumber(text);
        if (n === null || !Number.isInteger(n) || n < 16 || n > 16384) {
            return refuse("em is an integer from 16 to 16384.");
        }
        return { op: "fontField", name, value: { type: "num", value: n } };
    });
}
