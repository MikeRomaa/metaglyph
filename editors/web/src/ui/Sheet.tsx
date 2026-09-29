import type { ReactNode } from "react";
import { useStore } from "../state/store.ts";
import styles from "./sheet.module.css";

/** The left column: sheet-specific inspector over the GLOBALS panel. */
export function LeftColumn({ children }: { children: ReactNode }) {
    return (
        <aside className={styles.left}>
            <div className={styles.leftBody}>{children}</div>
            <Globals />
        </aside>
    );
}

function Globals() {
    const lets = useStore((s) => s.font?.lets);
    const select = useStore((s) => s.select);
    return (
        <div className={styles.globals}>
            <div className={styles.globalsHead}>
                <span>Globals</span>
                <span className={styles.globalsNote}>
                    top-level · locked to drags
                </span>
            </div>
            <div className={styles.globalsBody}>
                {lets?.map((l) => (
                    <button
                        type="button"
                        key={l.name}
                        className={styles.globalRow}
                        title={`let ${l.name} = ${l.expr}`}
                        onClick={() =>
                            select({
                                kind: "let",
                                name: l.name,
                                span: l.span,
                                origin: "canvas",
                            })
                        }
                    >
                        <span style={{ fontWeight: 600 }}>{l.name}</span>
                        <span style={{ color: "var(--mid)" }}>{l.expr}</span>
                        <span>{l.value}</span>
                    </button>
                ))}
            </div>
        </div>
    );
}

/** The centre column, with its toolbar and a banner while the drawing is
 * behind the text: read-only on syntax errors; still editable on errors
 * that only stop evaluation (an unresolved name). */
export function Centre({
    toolbar,
    children,
}: {
    toolbar: ReactNode;
    children: ReactNode;
}) {
    const parseOk = useStore((s) => s.doc?.parseOk ?? true);
    const evaluated = useStore((s) => s.doc?.evaluated ?? true);
    return (
        <main className={styles.centre}>
            <div className={styles.toolbar}>{toolbar}</div>
            {!parseOk ? (
                <div className={styles.banner}>
                    ▲ Syntax errors · showing the last good state · read-only
                </div>
            ) : (
                !evaluated && (
                    <div className={styles.banner}>
                        ▲ Errors in the source · showing the last state that
                        evaluated
                    </div>
                )
            )}
            {children}
        </main>
    );
}

export function Section({
    title,
    aside,
    flush,
    children,
}: {
    title: ReactNode;
    aside?: ReactNode;
    flush?: boolean;
    children?: ReactNode;
}) {
    return (
        <section
            className={`${styles.section} ${flush ? styles.sectionFlush : ""}`}
        >
            <div className={styles.sectionHead}>
                <span className="label">{title}</span>
                {aside !== undefined && (
                    <span className={styles.aside}>{aside}</span>
                )}
            </div>
            {children}
        </section>
    );
}

export function Ball({ n, on }: { n: number; on?: boolean }) {
    return (
        <span className={styles.ball} data-on={on || undefined}>
            {n}
        </span>
    );
}

export function Empty({ children }: { children: ReactNode }) {
    return (
        <div className={styles.empty}>
            <div className={styles.emptyCard}>{children}</div>
        </div>
    );
}

export { styles as sheet };
