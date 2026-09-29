import * as Comlink from "comlink";
import init, { Engine } from "../wasm/mg_web.js";
import type {
    DocState,
    EngineResult,
    FontData,
    GlyphScene,
    View,
} from "./types.ts";

const ready = init().then(() => new Engine());

/** The instances of the last text that parsed. */
let instances: string[] = [];

function view(
    engine: Engine,
    instance: string | null,
    glyph: string | null,
): View {
    const active =
        instance && instances.includes(instance)
            ? instance
            : (instances[0] ?? null);
    if (!active)
        return { instance: null, font: null, glyph: null, scene: null };
    const font = (engine.fontData(active) ?? null) as FontData | null;
    const names = font?.glyphs.map((g) => g.name) ?? [];
    const resolved =
        glyph && names.includes(glyph) ? glyph : (names[0] ?? null);
    const scene = resolved
        ? ((engine.glyphScene(active, resolved) ?? null) as GlyphScene | null)
        : null;
    return { instance: active, font, glyph: resolved, scene };
}

const api = {
    /** Checks `source`, then renders the view for `instance` / `glyph`. */
    async update(
        source: string,
        version: number,
        instance: string | null,
        glyph: string | null,
    ): Promise<EngineResult> {
        const engine = await ready;
        const doc = engine.update(source, version) as DocState;
        if (doc.parseOk) instances = doc.instances;
        return { doc, view: view(engine, instance, glyph) };
    },

    /** Renders the view for `instance` / `glyph` from the last good text. */
    async view(
        instance: string | null,
        glyph: string | null,
    ): Promise<EngineResult> {
        return { view: view(await ready, instance, glyph) };
    },
};

export type EngineApi = typeof api;

Comlink.expose(api);
