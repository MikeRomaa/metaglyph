// A canvas edit briefly flashes the text it changed (plan 5, §2.2).

import type { ChangeSet, Extension } from "@codemirror/state";
import { StateEffect, StateField } from "@codemirror/state";
import type { DecorationSet } from "@codemirror/view";
import { Decoration, EditorView } from "@codemirror/view";

const FLASH_MS = 900;

const addFlash = StateEffect.define<{ from: number; to: number }[]>();
const clearFlash = StateEffect.define<null>();
const mark = Decoration.mark({ class: "cm-mg-flash" });

const flashField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update(value, tr) {
        let next = value.map(tr.changes);
        for (const effect of tr.effects) {
            if (effect.is(clearFlash)) next = Decoration.none;
            if (effect.is(addFlash)) {
                next = Decoration.set(
                    effect.value.map((r) => mark.range(r.from, r.to)),
                    true,
                );
            }
        }
        return next;
    },
    provide: (field) => EditorView.decorations.from(field),
});

const flashTheme = EditorView.theme({
    "@keyframes cm-mg-flash": {
        from: { backgroundColor: "var(--tok)" },
        to: { backgroundColor: "transparent" },
    },
    ".cm-mg-flash": {
        animation: `cm-mg-flash ${FLASH_MS}ms ease-out`,
    },
});

export function flashExtension(): Extension {
    return [flashField, flashTheme];
}

/** The effect that flashes `changes`' inserted text (in the new document),
 * and schedules clearing it. */
export function flashEffect(view: EditorView, changes: ChangeSet) {
    const ranges: { from: number; to: number }[] = [];
    changes.iterChangedRanges((_fromA, _toA, fromB, toB) => {
        if (toB > fromB) ranges.push({ from: fromB, to: toB });
    });
    setTimeout(() => {
        view.dispatch({ effects: clearFlash.of(null) });
    }, FLASH_MS);
    return addFlash.of(ranges);
}
