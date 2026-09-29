import * as Comlink from "comlink";
import type { DocState } from "./types.ts";
import type { EngineApi } from "./worker.ts";

const worker = new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
});
const engine = Comlink.wrap<EngineApi>(worker);

let running = false;
let pending: { source: string; version: number } | null = null;
let listener: ((state: DocState) => void) | null = null;

/** Receives every check result, in order. Results may be for a version the
 * caller has since moved past; compare `state.version` before using spans. */
export function onDocState(fn: (state: DocState) => void) {
    listener = fn;
}

/** Queues a check of `source`. Only the newest queued text is checked once
 * the current run ends, so fast typing never builds a backlog. */
export function check(source: string, version: number) {
    pending = { source, version };
    if (!running) void drain();
}

async function drain() {
    running = true;
    while (pending) {
        const { source, version } = pending;
        pending = null;
        try {
            listener?.(await engine.update(source, version));
        } catch (error) {
            console.error("mg engine failed", error);
        }
    }
    running = false;
}
