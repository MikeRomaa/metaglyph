import type { ReactNode } from "react";
import styles from "./Frame.module.css";

const ZONES_X = ["8", "7", "6", "5", "4", "3", "2", "1"];
const ZONES_Y = ["A", "B", "C", "D", "E"];

/** The drawing sheet: zone rulers on all four edges around a 2px border. */
export function Frame({ children }: { children: ReactNode }) {
    return (
        <div className={styles.sheet}>
            <Ruler className={styles.top} zones={ZONES_X} />
            <Ruler className={styles.bottom} zones={ZONES_X} />
            <Ruler className={styles.left} zones={ZONES_Y} />
            <Ruler className={styles.right} zones={ZONES_Y} />
            <div className={styles.border}>{children}</div>
        </div>
    );
}

function Ruler({ className, zones }: { className: string; zones: string[] }) {
    return (
        <div className={`${styles.ruler} ${className}`} aria-hidden>
            {zones.map((z) => (
                <span key={z}>{z}</span>
            ))}
        </div>
    );
}
