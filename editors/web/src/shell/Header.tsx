import { useEffect, useRef, useState } from "react";
import {
    exportBlocked,
    exportDoc,
    exportTtf,
    newDoc,
    pickFile,
    replaceDoc,
    SAMPLES,
} from "../state/files.ts";
import { rememberTheme } from "../state/persist.ts";
import type { Theme } from "../state/store.ts";
import { SHEETS, useStore } from "../state/store.ts";
import styles from "./Header.module.css";

const THEMES: { label: string; value: Theme }[] = [
    { label: "Light", value: "light" },
    { label: "Dark", value: "dark" },
];

const NO_INSTANCES: string[] = [];

export function Header() {
    const fileName = useStore((s) => s.fileName);
    const sheet = useStore((s) => s.sheet);
    const setSheet = useStore((s) => s.setSheet);
    const theme = useStore((s) => s.theme);
    const setTheme = useStore((s) => s.setTheme);
    const instance = useStore((s) => s.instance);
    const setInstance = useStore((s) => s.setInstance);
    const instances = useStore((s) => s.lastGood?.instances ?? NO_INSTANCES);
    const doc = useStore((s) => s.doc);
    const blocked = exportBlocked(doc);
    const [exporting, setExporting] = useState(false);

    return (
        <header className={styles.bar}>
            <div className={styles.brand}>
                <span className={styles.wordmark}>Metaglyph</span>
                <FileMenu fileName={fileName} />
            </div>
            {SHEETS.map(({ n, label }) => (
                <button
                    type="button"
                    key={n}
                    className={styles.tab}
                    data-on={sheet === n || undefined}
                    onClick={() => setSheet(n)}
                >
                    <span className={styles.tabNo}>0{n}</span>
                    {label}
                </button>
            ))}
            <div className={styles.fill} />
            <label className={styles.instance}>
                Instance
                <select
                    value={instance ?? ""}
                    onChange={(e) => setInstance(e.target.value)}
                    disabled={instances.length === 0}
                >
                    {instances.map((name) => (
                        <option key={name} value={name}>
                            {name}
                        </option>
                    ))}
                </select>
            </label>
            <div className={styles.themes}>
                {THEMES.map(({ label, value }) => (
                    <button
                        type="button"
                        key={value}
                        className={styles.theme}
                        data-on={theme === value || undefined}
                        onClick={() => {
                            setTheme(value);
                            rememberTheme(value);
                        }}
                    >
                        {label}
                    </button>
                ))}
            </div>
            <button
                type="button"
                className={styles.export}
                disabled={blocked !== null || exporting}
                title={
                    blocked ??
                    (instances.length > 1
                        ? `Build ${instances.length} instances as a .zip of TTFs`
                        : "Build the font as TTF")
                }
                onClick={() => {
                    setExporting(true);
                    void exportTtf().finally(() => setExporting(false));
                }}
            >
                {exporting ? "Building…" : "Export TTF"}
            </button>
        </header>
    );
}

function FileMenu({ fileName }: { fileName: string }) {
    const [open, setOpen] = useState(false);
    const root = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!open) return;
        const close = (e: PointerEvent) => {
            if (!root.current?.contains(e.target as Node)) setOpen(false);
        };
        document.addEventListener("pointerdown", close);
        return () => document.removeEventListener("pointerdown", close);
    }, [open]);

    const run = (action: () => void) => () => {
        setOpen(false);
        action();
    };

    return (
        <div ref={root} className={styles.file}>
            <button
                type="button"
                className={styles.fileName}
                onClick={() => setOpen(!open)}
                aria-expanded={open}
            >
                {fileName} ▾
            </button>
            {open && (
                <div className={styles.menu} role="menu">
                    <button type="button" role="menuitem" onClick={run(newDoc)}>
                        New
                    </button>
                    <button
                        type="button"
                        role="menuitem"
                        onClick={run(pickFile)}
                    >
                        Import .mg…
                    </button>
                    <button
                        type="button"
                        role="menuitem"
                        onClick={run(exportDoc)}
                    >
                        Export .mg
                    </button>
                    <div className={styles.menuLabel}>Samples</div>
                    {SAMPLES.map((s) => (
                        <button
                            type="button"
                            key={s.fileName}
                            role="menuitem"
                            onClick={run(() => replaceDoc(s.fileName, s.text))}
                        >
                            {s.fileName}
                        </button>
                    ))}
                </div>
            )}
        </div>
    );
}
