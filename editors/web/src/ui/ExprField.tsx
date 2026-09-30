import { useState } from "react";
import styles from "./ExprField.module.css";

/** An expression typed into an inspector, committed on Enter or blur
 * (plan 5, §2.4); Escape reverts. Key it by `value` so a new value from
 * the source replaces the text. */
export function ExprField({
    value,
    placeholder,
    className,
    label,
    onCommit,
}: {
    value: string;
    placeholder?: string;
    className?: string;
    label?: string;
    onCommit: (text: string) => Promise<unknown>;
}) {
    const [text, setText] = useState(value);
    const commit = () => {
        const next = text.trim();
        if (next !== value) void onCommit(next);
    };
    return (
        <input
            className={`${styles.field} ${className ?? ""}`}
            value={text}
            placeholder={placeholder}
            aria-label={label}
            spellCheck={false}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") {
                    setText(value);
                    e.currentTarget.blur();
                }
            }}
            onBlur={commit}
        />
    );
}
