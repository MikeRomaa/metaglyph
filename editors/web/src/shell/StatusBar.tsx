import { fmt } from "../font/lookup.ts";
import type { Sheet } from "../state/store.ts";
import { useStore } from "../state/store.ts";
import styles from "./StatusBar.module.css";

const HINTS: Record<Sheet, string> = {
    1: "click or drag to pick · ⇧ range",
    2: "Tab next driver · Esc cancel",
    3: "⌘Z undo",
    4: "⌘Z undo",
};

/** `"Name <mail>"` → `"Name"`, as the design's title block shows it. */
function withoutEmail(designer: string) {
    return designer.replace(/\s*<[^>]*>\s*$/, "");
}

export function StatusBar() {
    const sheet = useStore((s) => s.sheet);
    const font = useStore((s) => s.lastGood?.font);
    const pointer = useStore((s) => s.pointer);
    const notice = useStore((s) => s.notice);

    return (
        <footer className={styles.bar}>
            <span className={styles.font}>{font?.name ?? "Untitled"}</span>
            <span className={styles.cell}>
                x {pointer ? fmt(pointer[0]) : "—"}
            </span>
            <span className={styles.cell}>
                y {pointer ? fmt(pointer[1]) : "—"}
            </span>
            <span className={styles.cell}>snap pts · lines · metrics</span>
            <span className={styles.cell}>grid 10</span>
            <div className={styles.fill} />
            {notice ? (
                <span
                    className={styles.end}
                    role="status"
                    style={{
                        color:
                            notice.tone === "error"
                                ? "var(--err)"
                                : "var(--ink)",
                    }}
                >
                    {notice.tone === "error" ? "▲ " : ""}
                    {notice.text}
                </span>
            ) : (
                <span className={styles.end}>{HINTS[sheet]}</span>
            )}
            {font?.designer && (
                <span className={styles.block}>
                    DESIGNER {withoutEmail(font.designer)}
                </span>
            )}
            <span className={styles.block}>REV {font?.version ?? "—"}</span>
            <span className={`${styles.block} ${styles.sheetNo}`}>
                SHEET 0{sheet} / 04
            </span>
        </footer>
    );
}
