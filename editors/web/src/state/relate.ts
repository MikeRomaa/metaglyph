// Relationship tools (plan 5, §1.4): each rewrites the selected point's
// `let` in terms of the points and lines picked on the canvas.

import type { GlyphScene, LineRef, Pt, Relation } from "../engine/types.ts";
import { performEdit } from "./actions.ts";
import type { RelateKind, RelatePick } from "./store.ts";
import { useStore } from "./store.ts";

type Need = "point" | "line";

export const RELATE_TOOLS: {
    kind: RelateKind;
    name: string;
    needs: Need[];
    prompt: string;
}[] = [
    {
        kind: "CO",
        name: "Coincident",
        needs: ["point"],
        prompt: "Click the point to coincide with.",
    },
    {
        kind: "IX",
        name: "Intersect",
        needs: ["line", "line"],
        prompt: "Click two lines to meet at.",
    },
    {
        kind: "PJ",
        name: "Project",
        needs: ["line"],
        prompt: "Click the line to project onto.",
    },
    {
        kind: "FR",
        name: "Fraction",
        needs: ["point", "point"],
        prompt: "Click the two points to sit between.",
    },
    {
        kind: "PL",
        name: "Polar",
        needs: ["point"],
        prompt: "Click the centre point.",
    },
    {
        kind: "MR",
        name: "Mirror",
        needs: ["point", "line"],
        prompt: "Click the point to mirror, then the axis.",
    },
];

/** Lines a pick can use: named construction lines, and straight segments
 * between named points. */
export function pickableLines(scene: GlyphScene): number {
    const named = scene.lines.filter((l) => !l.of).length;
    const segments = scene.paths
        .flatMap((p) =>
            p.segments.map((s, i) => ({ s, prev: p.segments[i - 1] })),
        )
        .filter(
            ({ s, prev }) => s.kind === "line" && s.toRef && prev?.toRef,
        ).length;
    return named + segments;
}

/** Why a tool is disabled for the selected point, or null. */
export function disabledReason(
    kind: RelateKind,
    scene: GlyphScene,
): string | null {
    const tool = RELATE_TOOLS.find((t) => t.kind === kind);
    if (tool?.needs.includes("line") && pickableLines(scene) === 0) {
        return "No named line or straight segment to use in this glyph.";
    }
    return null;
}

export function startRelate(kind: RelateKind, target: string) {
    const tool = RELATE_TOOLS.find((t) => t.kind === kind);
    if (!tool) return;
    useStore.getState().setRelate({ kind, target, picks: [] });
    useStore
        .getState()
        .setNotice(`${tool.name}: ${tool.prompt} Esc cancels.`, "info");
}

export function cancelRelate() {
    useStore.getState().setRelate(null);
}

/** Adds a pick; applies the tool once it has all it needs. */
export function relatePick(pick: RelatePick) {
    const s = useStore.getState();
    const relate = s.relate;
    const scene = s.scene;
    if (!relate || !scene) return;
    const tool = RELATE_TOOLS.find((t) => t.kind === relate.kind);
    if (!tool) return;
    const need = tool.needs[relate.picks.length];
    if (pick.type !== need) {
        s.setNotice(`${tool.name}: pick a ${need} here.`, "info");
        return;
    }
    if (pick.type === "point" && pick.name === relate.target) {
        s.setNotice(
            `${tool.name}: pick a point other than ${relate.target}.`,
            "info",
        );
        return;
    }
    const picks = [...relate.picks, pick];
    if (picks.length < tool.needs.length) {
        s.setRelate({ ...relate, picks });
        return;
    }
    s.setRelate(null);
    const relation = build(relate.kind, relate.target, picks, scene);
    if (!relation) return;
    void performEdit(
        `relate_${relate.kind.toLowerCase()}`,
        () => {
            const span = useStore
                .getState()
                .scene?.points.find((p) => p.name === relate.target)?.span;
            return span ? { op: "relate", span, relation } : null;
        },
        { rename: false },
    );
}

function point(scene: GlyphScene, name: string): Pt | undefined {
    return scene.points.find((p) => p.name === name)?.at;
}

/** The relation for complete `picks`, computing its numbers from where the
 * target is now (plan 5: `t` of its projection onto `ab`; `len` and `θ`
 * from the centre). */
function build(
    kind: RelateKind,
    target: string,
    picks: RelatePick[],
    scene: GlyphScene,
): Relation | null {
    const at = point(scene, target);
    if (!at) return null;
    const name = (i: number) => (picks[i] as { name: string }).name;
    const line = (i: number): LineRef => (picks[i] as { line: LineRef }).line;
    switch (kind) {
        case "CO":
            return { kind: "coincident", b: name(0) };
        case "IX":
            return { kind: "meet", l1: line(0), l2: line(1) };
        case "PJ":
            return { kind: "project", at, line: line(0) };
        case "FR": {
            const a = point(scene, name(0));
            const b = point(scene, name(1));
            if (!a || !b) return null;
            const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
            const len2 = dx * dx + dy * dy;
            const t =
                len2 === 0
                    ? 0
                    : ((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / len2;
            return { kind: "fraction", a: name(0), b: name(1), t };
        }
        case "PL": {
            const q = point(scene, name(0));
            if (!q) return null;
            const [dx, dy] = [at[0] - q[0], at[1] - q[1]];
            const angle = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360;
            return {
                kind: "polar",
                q: name(0),
                len: Math.hypot(dx, dy),
                angle,
            };
        }
        case "MR":
            return { kind: "mirror", q: name(0), axis: line(1) };
    }
}
