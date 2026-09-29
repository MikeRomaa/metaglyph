import babel from "@rolldown/plugin-babel";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
    plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
    worker: {
        format: "es",
    },
    server: {
        fs: {
            // The bundled samples live in the repo's samples/ directory.
            allow: ["../.."],
        },
    },
});
