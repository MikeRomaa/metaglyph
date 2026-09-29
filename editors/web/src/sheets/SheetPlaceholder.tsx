import type { Sheet } from "../state/store.ts";
import { useStore } from "../state/store.ts";
import styles from "./SheetPlaceholder.module.css";

const VIEWS: Record<Sheet, { view: string; left: string }> = {
    1: { view: "View 01 · Charset", left: "Character sets" },
    2: { view: "View 02 · Glyph", left: "Selection" },
    3: { view: "View 03 · Spacing", left: "Spacing" },
    4: { view: "View 04 · Kerning", left: "Groups" },
};

/** Left column and centre for a sheet whose drawing arrives in W2. */
export function SheetPlaceholder({ sheet }: { sheet: Sheet }) {
    const doc = useStore((s) => s.doc);
    const lastGood = useStore((s) => s.lastGood);
    const { view, left } = VIEWS[sheet];

    return (
        <>
            <aside className={styles.left}>
                <div className={styles.leftBody}>
                    <div className={styles.section}>
                        <span className="label">{left}</span>
                        <p className="note">Inspector arrives in W2.</p>
                    </div>
                </div>
                <div className={styles.globals}>
                    <span>Globals</span>
                    <span className={styles.globalsNote}>
                        top-level · locked to drags
                    </span>
                </div>
            </aside>
            <main className={styles.centre}>
                <div className={styles.toolbar}>
                    <span className={styles.view}>{view}</span>
                </div>
                <div className={`${styles.body} hatched`}>
                    <div className={styles.card}>
                        {doc && !doc.parseOk && (
                            <p className={styles.error}>
                                ▲ Syntax errors: showing the last good state,
                                read-only.
                            </p>
                        )}
                        <p className="note">
                            {lastGood
                                ? `${lastGood.glyphCount} glyph${lastGood.glyphCount === 1 ? "" : "s"} · ${lastGood.instances.length} instance${lastGood.instances.length === 1 ? "" : "s"}. Drawing arrives in W2.`
                                : "Checking…"}
                        </p>
                    </div>
                </div>
            </main>
        </>
    );
}
