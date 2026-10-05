import * as Comlink from "comlink";
import init, { Engine } from "../wasm/mg_web.js";
import type {
    BuildResult,
    CompletionInfo,
    DocState,
    DragInfo,
    DragStep,
    EditResult,
    EngineResult,
    FontData,
    FormatResult,
    GlyphScene,
    Op,
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
        // The views come from the last text that evaluated; so do its
        // instances.
        if (doc.evaluated) instances = doc.instances;
        return { doc, view: view(engine, instance, glyph) };
    },

    /** The completions at UTF-16 `offset` in `source`. */
    async complete(source: string, offset: number): Promise<CompletionInfo[]> {
        return (await ready).complete(source, offset) as CompletionInfo[];
    },

    /** Runs an edit op against document `version`. */
    async edit(op: Op, version: number): Promise<EditResult> {
        return (await ready).edit(op, version) as EditResult;
    },

    /** Pins document `version` as an edit gesture's start: edits keep
     * running against it until `unpin`. False if it isn't current. */
    async pin(version: number): Promise<boolean> {
        return (await ready).pin(version);
    },

    async unpin(): Promise<void> {
        (await ready).unpin();
    },

    /** What dragging `target` would change; null if it can't be dragged. */
    async drivers(
        instance: string,
        glyph: string,
        target: string,
    ): Promise<DragInfo | null> {
        return ((await ready).drivers(instance, glyph, target) ??
            null) as DragInfo | null;
    },

    /** Starts a point drag on document `version`. */
    async dragBegin(
        instance: string,
        glyph: string,
        target: string,
        version: number,
    ): Promise<DragInfo | null> {
        return ((await ready).dragBegin(instance, glyph, target, version) ??
            null) as DragInfo | null;
    },

    async dragTo(x: number, y: number): Promise<DragStep | null> {
        return ((await ready).dragTo(x, y) ?? null) as DragStep | null;
    },

    async dragSet(driver: number, value: number): Promise<DragStep | null> {
        return ((await ready).dragSet(driver, value) ??
            null) as DragStep | null;
    },

    async dragCycle(axis: number): Promise<DragInfo | null> {
        return ((await ready).dragCycle(axis) ?? null) as DragInfo | null;
    },

    async dragEnd(glyph: string): Promise<void> {
        (await ready).dragEnd(glyph);
    },

    /** `source` formatted as `mg fmt` does. */
    async format(source: string): Promise<FormatResult> {
        return (await ready).format(source) as FormatResult;
    },

    /** Builds every instance as TTF; null if the text has errors. The
     * font data is transferred, not copied. */
    async buildTtf(timestamp: number): Promise<BuildResult | null> {
        const result = ((await ready).buildTtf(timestamp) ??
            null) as BuildResult | null;
        if (!result) return null;
        return Comlink.transfer(
            result,
            result.fonts.map((f) => f.data.buffer as ArrayBuffer),
        );
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
