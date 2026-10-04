import { useState } from "react";
import type { FontData } from "../../engine/types.ts";
import { charCode } from "../../font/chars.ts";
import { sampleChar } from "../../font/lookup.ts";
import {
    addGlyphs,
    defaultAdvance,
    glyphDecl,
    type NewGlyph,
    nameErrors,
    newGlyph,
} from "../../state/glyphs.ts";
import { Modal, modal } from "../../ui/Modal.tsx";
import styles from "./AddGlyphsModal.module.css";

/**
 * The picked codepoints as new glyphs (plan 6, §4 "01 GLYPHS"): each named
 * from the AGLFN (or `uniXXXX`), editable and checked as typed, with the
 * declarations previewed. INSERT adds them all as one undo step.
 */
export function AddGlyphsModal({
    font,
    codepoints,
    onClose,
    onAdded,
}: {
    font: FontData;
    codepoints: number[];
    onClose: () => void;
    onAdded: (names: string[]) => void;
}) {
    /** The picks, ascending; row `i` is `ids[i]`. */
    const [ids] = useState(() => [...codepoints].sort((a, b) => a - b));
    const [rows, setRows] = useState<NewGlyph[]>(() => ids.map(newGlyph));
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const errors = nameErrors(rows, font);
    const invalid = errors.filter(Boolean).length;
    const ready = invalid === 0 && rows.length > 0 && !busy;

    const submit = async () => {
        if (!ready) return;
        setBusy(true);
        const result = await addGlyphs(rows);
        setBusy(false);
        if (result?.status === "ok") {
            onAdded(rows.map((r) => r.name));
            onClose();
        } else if (result?.status === "invalid") {
            setError(result.message);
        }
    };

    const n = rows.length;
    return (
        <Modal
            title={`Add ${n} glyph${n === 1 ? "" : "s"}`}
            aside="names from the AGLFN · uniXXXX otherwise · editable"
            onClose={onClose}
            footer={
                <>
                    {error ? (
                        <p className={modal.error}>{error}</p>
                    ) : invalid > 0 ? (
                        <p className={modal.error}>
                            {invalid} name{invalid === 1 ? "" : "s"} can't be
                            used yet.
                        </p>
                    ) : (
                        <span className={modal.hint}>
                            Each gets{" "}
                            <code>advance: {defaultAdvance(font)}</code> and an
                            empty body. Inserting is one undo step.
                        </span>
                    )}
                    <button
                        type="button"
                        className={modal.button}
                        onClick={onClose}
                    >
                        Cancel
                    </button>
                    <button
                        type="button"
                        className={`${modal.button} ${modal.primary}`}
                        disabled={!ready}
                        onClick={() => void submit()}
                    >
                        Insert {n} ↵
                    </button>
                </>
            }
        >
            <div className={styles.body}>
                <div className={styles.rows}>
                    <div className={styles.head}>
                        <span />
                        <span>Codepoint</span>
                        <span>Name</span>
                    </div>
                    {rows.map((row, i) => (
                        <div
                            key={ids[i]}
                            className={styles.row}
                            data-error={errors[i] ? true : undefined}
                        >
                            <span className={styles.char}>
                                {sampleChar(ids[i])}
                            </span>
                            <span className={styles.cp}>
                                {charCode(ids[i])}
                            </span>
                            <span className={styles.nameCell}>
                                <input
                                    className={styles.name}
                                    value={row.name}
                                    aria-label={`Name for ${charCode(ids[i])}`}
                                    aria-invalid={errors[i] ? true : undefined}
                                    spellCheck={false}
                                    // biome-ignore lint/a11y/noAutofocus: the first name is what to check first
                                    autoFocus={i === 0}
                                    onChange={(e) => {
                                        const next = [...rows];
                                        next[i] = {
                                            ...row,
                                            name: e.target.value.trim(),
                                        };
                                        setRows(next);
                                        setError(null);
                                    }}
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter") void submit();
                                    }}
                                />
                                {errors[i] && (
                                    <span className={styles.why}>
                                        {errors[i]}
                                    </span>
                                )}
                            </span>
                        </div>
                    ))}
                </div>
                <div className={styles.preview}>
                    <div className={styles.previewHead}>Preview</div>
                    <pre className={styles.code}>
                        {rows
                            .slice(0, 2)
                            .map((row) => glyphDecl(row, defaultAdvance(font)))
                            .join("\n\n")}
                        {n > 2 && `\n\n// … ${n - 2} more`}
                    </pre>
                </div>
            </div>
        </Modal>
    );
}
