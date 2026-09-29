import { openDB } from "idb";
import type { Theme } from "./store.ts";
import { useStore } from "./store.ts";

// One project (plan 6: no library yet), stored under a fixed key.
const DB = "metaglyph";
const STORE = "docs";
const KEY = "current";
const AUTOSAVE_MS = 1000;

interface SavedDoc {
    fileName: string;
    text: string;
    updatedAt: number;
}

const db = openDB(DB, 1, {
    upgrade(db) {
        db.createObjectStore(STORE);
    },
});

export async function loadSaved(): Promise<SavedDoc | undefined> {
    try {
        return await (await db).get(STORE, KEY);
    } catch (error) {
        console.error("could not read the saved document", error);
        return undefined;
    }
}

async function save() {
    const { fileName, text, version } = useStore.getState();
    const doc: SavedDoc = { fileName, text, updatedAt: Date.now() };
    try {
        await (await db).put(STORE, doc, KEY);
        // Only "saved" if nothing changed while writing.
        if (useStore.getState().version === version)
            useStore.getState().setSaved(true);
    } catch (error) {
        console.error("could not save the document", error);
    }
}

/** Saves the document 1 s after the last change. */
export function startAutosave() {
    let timer: ReturnType<typeof setTimeout> | undefined;
    return useStore.subscribe((s, prev) => {
        if (s.version === prev.version && s.fileName === prev.fileName) return;
        clearTimeout(timer);
        timer = setTimeout(save, AUTOSAVE_MS);
    });
}

const THEME_KEY = "metaglyph.theme";

export function initialTheme(): Theme {
    try {
        const stored = localStorage.getItem(THEME_KEY);
        if (stored === "light" || stored === "dark") return stored;
    } catch {
        // Storage blocked; fall back to the system preference.
    }
    return matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
}

export function rememberTheme(theme: Theme) {
    try {
        localStorage.setItem(THEME_KEY, theme);
    } catch {
        // Not remembered; harmless.
    }
}
