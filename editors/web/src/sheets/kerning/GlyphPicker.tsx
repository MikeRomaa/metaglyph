// Picking glyphs for kerning: a searchable grid of glyph tiles (as on the
// 01 GLYPHS sheet) and the font's groups. A new pair picks each side; a
// side of several glyphs becomes a new group declared with the pair. A
// group's `+` picks glyphs to add to it.

import { type MouseEvent, type ReactNode, useState } from "react";
import type {
    FontData,
    GlyphInfo,
    GroupInfo,
    KernSideSpec,
} from "../../engine/types.ts";
import { hex, verticalExtent } from "../../font/lookup.ts";
import { groupMember, newKern } from "../../state/kerning.ts";
import { GlyphThumb } from "../../ui/GlyphThumb.tsx";
import { Modal, modal } from "../../ui/Modal.tsx";
import styles from "./GlyphPicker.module.css";

/** Whether a glyph matches a search: its name, its character, or its
 * codepoint in hex (`41`, `U+0041`). */
export function matchesGlyph(glyph: GlyphInfo, query: string): boolean {
    const q = query.trim();
    if (!q) return true;
    if (glyph.name.toLowerCase().includes(q.toLowerCase())) return true;
    if (glyph.codepoints.some((cp) => String.fromCodePoint(cp) === q)) {
        return true;
    }
    const code = q.replace(/^u\+/i, "").toUpperCase();
    return (
        /^[0-9A-F]{2,6}$/.test(code) &&
        glyph.codepoints.some((cp) => hex(cp) === code.padStart(4, "0"))
    );
}

/** Several glyphs are picked with ⌘/Ctrl- or ⇧-click. */
function isMulti(e: MouseEvent) {
    return e.metaKey || e.ctrlKey || e.shiftKey;
}

function Grid({
    font,
    query,
    picked,
    pickedGroup,
    disabled,
    showGroups,
    onGlyph,
    onGroup,
}: {
    font: FontData;
    query: string;
    picked: string[];
    pickedGroup?: string;
    disabled?: Set<string>;
    showGroups: boolean;
    onGlyph: (name: string, e: MouseEvent) => void;
    onGroup?: (name: string) => void;
}) {
    const [descender, ascender] = verticalExtent(font);
    const glyphs = font.glyphs.filter((g) => matchesGlyph(g, query));
    const byName = new Map(font.glyphs.map((g) => [g.name, g]));
    const groups = showGroups
        ? font.groups.filter(
              (g) =>
                  g.name.toLowerCase().includes(query.trim().toLowerCase()) ||
                  g.glyphs.some((m) => {
                      const glyph = byName.get(m);
                      return glyph !== undefined && matchesGlyph(glyph, query);
                  }),
          )
        : [];
    const thumb = (g: GlyphInfo) => (
        <GlyphThumb
            key={g.name}
            className={styles.thumb}
            glyph={g}
            ascender={ascender}
            descender={descender}
        />
    );

    return (
        <div className={styles.scroll}>
            {groups.length > 0 && (
                <>
                    <div className={styles.heading}>Groups</div>
                    <div className={styles.grid}>
                        {groups.map((group) => (
                            <GroupTile
                                key={group.name}
                                group={group}
                                on={group.name === pickedGroup}
                                thumbs={group.glyphs
                                    .slice(0, 3)
                                    .map((m) => byName.get(m))
                                    .filter((g) => g !== undefined)
                                    .map(thumb)}
                                onClick={() => onGroup?.(group.name)}
                            />
                        ))}
                    </div>
                </>
            )}
            <div className={styles.heading}>
                Glyphs · {glyphs.length}
                {query.trim() ? ` matching “${query.trim()}”` : ""}
            </div>
            <div className={styles.grid}>
                {glyphs.map((g) => {
                    const off = disabled?.has(g.name);
                    const on = picked.includes(g.name);
                    return (
                        <button
                            type="button"
                            key={g.name}
                            className={styles.tile}
                            data-state={off ? "off" : on ? "on" : undefined}
                            disabled={off}
                            title={`${g.name}${g.codepoints.length ? ` · U+${hex(g.codepoints[0])}` : ""}`}
                            onClick={(e) => onGlyph(g.name, e)}
                        >
                            <span className={styles.art}>{thumb(g)}</span>
                            <span className={styles.meta}>
                                <span>{g.name}</span>
                                <span>
                                    {g.codepoints.length
                                        ? hex(g.codepoints[0])
                                        : "—"}
                                </span>
                            </span>
                        </button>
                    );
                })}
                {glyphs.length === 0 && (
                    <p className={styles.none}>No glyph matches.</p>
                )}
            </div>
        </div>
    );
}

function GroupTile({
    group,
    on,
    thumbs,
    onClick,
}: {
    group: GroupInfo;
    on: boolean;
    thumbs: ReactNode[];
    onClick: () => void;
}) {
    return (
        <button
            type="button"
            className={`${styles.tile} ${styles.groupTile}`}
            data-state={on ? "on" : undefined}
            title={`${group.name}: ${group.glyphs.join(" ")}`}
            onClick={onClick}
        >
            <span className={styles.art}>{thumbs}</span>
            <span className={styles.meta}>
                <span>@{group.name}</span>
                <span>{group.glyphs.length}</span>
            </span>
        </button>
    );
}

// ---------------------------------------------------------------------
// New pair

/** One side being picked: glyphs (several make a new group, which needs
 * a name) or an existing group. */
type Side =
    | { kind: "glyphs"; names: string[]; group: string }
    | { kind: "group"; name: string }
    | null;

function spec(side: Side): KernSideSpec | null {
    if (!side) return null;
    if (side.kind === "group") return side.name;
    if (side.names.length === 1) return side.names[0];
    const group = side.group.trim();
    return group ? { group, glyphs: side.names } : null;
}

export function PairPicker({
    font,
    onClose,
}: {
    font: FontData;
    onClose: () => void;
}) {
    const [sides, setSides] = useState<[Side, Side]>([null, null]);
    const [active, setActive] = useState<0 | 1>(0);
    const [query, setQuery] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const current = sides[active];
    const setSide = (side: Side, advance: boolean) => {
        const next: [Side, Side] = [...sides];
        next[active] = side;
        setSides(next);
        setError(null);
        // A single pick on the left moves on to the right.
        if (advance && active === 0 && !next[1]) {
            setActive(1);
            setQuery("");
        }
    };
    const onGlyph = (name: string, e: MouseEvent) => {
        if (!isMulti(e)) {
            setSide({ kind: "glyphs", names: [name], group: "" }, true);
            return;
        }
        const names =
            current?.kind === "glyphs"
                ? current.names.includes(name)
                    ? current.names.filter((n) => n !== name)
                    : [...current.names, name]
                : [name];
        setSide(
            names.length
                ? {
                      kind: "glyphs",
                      names,
                      group: current?.kind === "glyphs" ? current.group : "",
                  }
                : null,
            false,
        );
    };

    const left = spec(sides[0]);
    const right = spec(sides[1]);
    const ready = left !== null && right !== null && !busy;
    const submit = async () => {
        if (left === null || right === null) return;
        setBusy(true);
        const result = await newKern(left, right);
        setBusy(false);
        if (result?.status === "ok") onClose();
        else if (result?.status === "invalid") setError(result.message);
    };

    return (
        <Modal
            title="New kern pair"
            aside="Pick the left side, then the right"
            onClose={onClose}
            footer={
                <>
                    {error ? (
                        <p className={modal.error}>{error}</p>
                    ) : (
                        <span className={modal.hint}>
                            Click picks one glyph or group · ⌘/Ctrl- or ⇧-click
                            picks several, declared as a new group
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
                        Add pair ↵
                    </button>
                </>
            }
        >
            <div className={styles.slots}>
                {([0, 1] as const).map((i) => (
                    <SideSlot
                        key={i}
                        label={i === 0 ? "Left" : "Right"}
                        side={sides[i]}
                        on={active === i}
                        onActivate={() => setActive(i)}
                        onName={(group) => {
                            const side = sides[i];
                            if (side?.kind !== "glyphs") return;
                            const next: [Side, Side] = [...sides];
                            next[i] = { ...side, group };
                            setSides(next);
                            setError(null);
                        }}
                        onClear={() => {
                            const next: [Side, Side] = [...sides];
                            next[i] = null;
                            setSides(next);
                            setActive(i);
                        }}
                    />
                ))}
            </div>
            <Search
                value={query}
                onChange={setQuery}
                onEnter={() => {
                    if (ready) void submit();
                }}
            />
            <Grid
                font={font}
                query={query}
                picked={current?.kind === "glyphs" ? current.names : []}
                pickedGroup={
                    current?.kind === "group" ? current.name : undefined
                }
                showGroups
                onGlyph={onGlyph}
                onGroup={(name) => setSide({ kind: "group", name }, true)}
            />
        </Modal>
    );
}

function SideSlot({
    label,
    side,
    on,
    onActivate,
    onName,
    onClear,
}: {
    label: string;
    side: Side;
    on: boolean;
    onActivate: () => void;
    onName: (name: string) => void;
    onClear: () => void;
}) {
    const many = side?.kind === "glyphs" && side.names.length > 1;
    return (
        // biome-ignore lint/a11y/useSemanticElements: holds an input, so it can't be a <button>
        <div
            className={styles.slot}
            data-on={on || undefined}
            role="button"
            tabIndex={0}
            onClick={onActivate}
            onKeyDown={(e) => {
                if (e.target === e.currentTarget && e.key === "Enter")
                    onActivate();
            }}
        >
            <span className={styles.slotLabel}>{label}</span>
            <span className={styles.slotValue}>
                {!side && <span className={styles.faint}>pick a glyph</span>}
                {side?.kind === "group" && `@${side.name}`}
                {side?.kind === "glyphs" && !many && side.names[0]}
                {many && side?.kind === "glyphs" && (
                    <>
                        <input
                            className={styles.groupName}
                            placeholder="new group name"
                            aria-label={`${label} group name`}
                            spellCheck={false}
                            value={side.group}
                            onClick={(e) => e.stopPropagation()}
                            onChange={(e) => onName(e.target.value)}
                        />
                        <span className={styles.members}>
                            {side.names.join(" ")}
                        </span>
                    </>
                )}
            </span>
            {side && (
                <button
                    type="button"
                    className={styles.clear}
                    aria-label={`Clear ${label}`}
                    onClick={(e) => {
                        e.stopPropagation();
                        onClear();
                    }}
                >
                    ×
                </button>
            )}
        </div>
    );
}

function Search({
    value,
    onChange,
    onEnter,
}: {
    value: string;
    onChange: (value: string) => void;
    onEnter: () => void;
}) {
    return (
        <div className={styles.searchRow}>
            <span className={styles.searchLabel}>Search</span>
            <input
                className={styles.search}
                value={value}
                placeholder="name, character, or U+0041"
                aria-label="Search glyphs"
                spellCheck={false}
                // biome-ignore lint/a11y/noAutofocus: the modal opens to search
                autoFocus
                onChange={(e) => onChange(e.target.value)}
                onKeyDown={(e) => {
                    if (e.key === "Enter") onEnter();
                }}
            />
        </div>
    );
}

// ---------------------------------------------------------------------
// Group members

export function MemberPicker({
    font,
    group,
    onClose,
}: {
    font: FontData;
    group: GroupInfo;
    onClose: () => void;
}) {
    const [picked, setPicked] = useState<string[]>([]);
    const [query, setQuery] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const submit = async () => {
        if (picked.length === 0) return;
        setBusy(true);
        const result = await groupMember(group.name, picked, true);
        setBusy(false);
        if (result?.status === "ok") onClose();
        else if (result?.status === "invalid") setError(result.message);
    };
    return (
        <Modal
            title={`Add to group ${group.name}`}
            aside={`${group.glyphs.join(" ")}`}
            onClose={onClose}
            footer={
                <>
                    {error ? (
                        <p className={modal.error}>{error}</p>
                    ) : (
                        <span className={modal.hint}>
                            {picked.length
                                ? `Adding ${picked.join(" ")}`
                                : "Click glyphs to add them"}
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
                        disabled={picked.length === 0 || busy}
                        onClick={() => void submit()}
                    >
                        {picked.length > 1
                            ? `Add ${picked.length} glyphs ↵`
                            : "Add glyph ↵"}
                    </button>
                </>
            }
        >
            <Search
                value={query}
                onChange={setQuery}
                onEnter={() => void submit()}
            />
            <Grid
                font={font}
                query={query}
                picked={picked}
                disabled={new Set(group.glyphs)}
                showGroups={false}
                onGlyph={(name) => {
                    setError(null);
                    setPicked(
                        picked.includes(name)
                            ? picked.filter((n) => n !== name)
                            : [...picked, name],
                    );
                }}
            />
        </Modal>
    );
}
