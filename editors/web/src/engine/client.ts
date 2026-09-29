import * as Comlink from "comlink";
import { useStore } from "../state/store.ts";
import type { EngineResult } from "./types.ts";
import type { EngineApi } from "./worker.ts";

const worker = new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
});
const engine = Comlink.wrap<EngineApi>(worker);

let running = false;
let pendingText: { source: string; version: number } | null = null;
let pendingView = false;
let listener: ((result: EngineResult) => void) | null = null;

/** Receives every result, in order. A result's `doc` may be for a version
 * the caller has since moved past; compare `doc.version` before using its
 * spans. */
export function onResult(fn: (result: EngineResult) => void) {
    listener = fn;
}

/** Queues a check of `source`. Only the newest queued text is checked once
 * the current run ends, so fast typing never builds a backlog. */
export function check(source: string, version: number) {
    pendingText = { source, version };
    if (!running) void drain();
}

/** Queues a re-render of the view (the active instance or glyph changed). */
export function requestView() {
    pendingView = true;
    if (!running) void drain();
}

async function drain() {
    running = true;
    while (pendingText || pendingView) {
        const text = pendingText;
        pendingText = null;
        pendingView = false;
        // Read the view arguments at send time, so they are never stale.
        const { instance, glyph } = useStore.getState();
        try {
            const result = text
                ? await engine.update(
                      text.source,
                      text.version,
                      instance,
                      glyph,
                  )
                : await engine.view(instance, glyph);
            listener?.(result);
        } catch (error) {
            console.error("mg engine failed", error);
        }
    }
    running = false;
}

// The view follows the active instance and glyph.
useStore.subscribe((s, prev) => {
    if (s.instance !== prev.instance || s.glyph !== prev.glyph) requestView();
});
