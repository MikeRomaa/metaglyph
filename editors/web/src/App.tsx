import { useEffect } from "react";
import styles from "./App.module.css";
import { GlyphSheet } from "./sheets/glyph/GlyphSheet.tsx";
import { GlyphsSheet } from "./sheets/glyphs/GlyphsSheet.tsx";
import { KerningSheet } from "./sheets/kerning/KerningSheet.tsx";
import { PreviewSheet } from "./sheets/preview/PreviewSheet.tsx";
import { SpacingSheet } from "./sheets/spacing/SpacingSheet.tsx";
import { Frame } from "./shell/Frame.tsx";
import { Header } from "./shell/Header.tsx";
import { ReplaceModal } from "./shell/ReplaceModal.tsx";
import { StatusBar } from "./shell/StatusBar.tsx";
import { SourcePane } from "./source/SourcePane.tsx";
import { importFile } from "./state/files.ts";
import { useEditShortcuts } from "./state/shortcuts.ts";
import { useStore } from "./state/store.ts";

export default function App() {
    const sheet = useStore((s) => s.sheet);
    const theme = useStore((s) => s.theme);
    useEditShortcuts();

    useEffect(() => {
        document.documentElement.dataset.theme = theme;
    }, [theme]);

    // Dropping a .mg file anywhere imports it (plan 5, §2.1); other files
    // are refused, and unexported work is confirmed before it's replaced.
    useEffect(() => {
        const over = (e: DragEvent) => e.preventDefault();
        const drop = (e: DragEvent) => {
            e.preventDefault();
            const file = e.dataTransfer?.files[0];
            if (file) void importFile(file);
        };
        window.addEventListener("dragover", over);
        window.addEventListener("drop", drop);
        return () => {
            window.removeEventListener("dragover", over);
            window.removeEventListener("drop", drop);
        };
    }, []);

    return (
        <Frame>
            <Header />
            <div className={styles.body}>
                {sheet === 1 && <GlyphsSheet />}
                {sheet === 2 && <GlyphSheet />}
                {sheet === 3 && <SpacingSheet />}
                {sheet === 4 && <KerningSheet />}
                {sheet === 5 && <PreviewSheet />}
                <SourcePane />
            </div>
            <StatusBar />
            <ReplaceModal />
        </Frame>
    );
}
