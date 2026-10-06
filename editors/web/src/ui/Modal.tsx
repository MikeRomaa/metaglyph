import { type ReactNode, useEffect } from "react";
import styles from "./Modal.module.css";

/** A modal sheet over a hatched backdrop (the design's `Modal`). Escape
 * and a click on the backdrop close it; Escape never reaches the editor's
 * own shortcuts. */
export function Modal({
    title,
    aside,
    footer,
    compact,
    onClose,
    children,
}: {
    title: ReactNode;
    aside?: ReactNode;
    footer?: ReactNode;
    /** Sized to its content, for a short question. */
    compact?: boolean;
    onClose: () => void;
    children: ReactNode;
}) {
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.key !== "Escape") return;
            e.stopImmediatePropagation();
            onClose();
        };
        window.addEventListener("keydown", onKey, true);
        return () => window.removeEventListener("keydown", onKey, true);
    }, [onClose]);

    return (
        // biome-ignore lint/a11y/noStaticElementInteractions: the backdrop; Escape is the keyboard path
        <div
            className={styles.backdrop}
            onMouseDown={(e) => {
                if (e.target === e.currentTarget) onClose();
            }}
        >
            <div
                className={styles.sheet}
                data-compact={compact || undefined}
                role="dialog"
                aria-modal="true"
            >
                <div className={styles.head}>
                    <span className={styles.title}>{title}</span>
                    {aside !== undefined && (
                        <span className={styles.aside}>{aside}</span>
                    )}
                    <button
                        type="button"
                        className={styles.close}
                        aria-label="Close"
                        onClick={onClose}
                    >
                        ×
                    </button>
                </div>
                <div className={styles.body}>{children}</div>
                {footer && <div className={styles.foot}>{footer}</div>}
            </div>
        </div>
    );
}

export { styles as modal };
