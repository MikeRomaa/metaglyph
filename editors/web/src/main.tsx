import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "@fontsource/ibm-plex-sans-condensed/400.css";
import "@fontsource/ibm-plex-sans-condensed/500.css";
import "@fontsource/ibm-plex-sans-condensed/600.css";
import "@fontsource/ibm-plex-sans-condensed/700.css";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
// Placeholders for characters a font lacks (01 GLYPHS): Noto's script and
// symbol families, each loaded per subset only when a sheet shows it.
import "@fontsource/noto-sans/400.css";
import "@fontsource/noto-sans-math/400.css";
import "@fontsource/noto-sans-symbols/400.css";
import "@fontsource/noto-sans-symbols-2/400.css";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/type.css";
import App from "./App.tsx";
import { SKELETON } from "./state/files.ts";
import { initialTheme, loadSaved, startAutosave } from "./state/persist.ts";
import { useStore } from "./state/store.ts";

const theme = initialTheme();
useStore.getState().setTheme(theme);
document.documentElement.dataset.theme = theme;

const saved = await loadSaved();
// A save from before export tracking counts as never exported.
if (saved)
    useStore
        .getState()
        .openDoc(saved.fileName, saved.text, saved.exportedText ?? null);
else useStore.getState().openDoc("untitled.mg", SKELETON);
startAutosave();

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root element");

createRoot(root).render(
    <StrictMode>
        <App />
    </StrictMode>,
);
