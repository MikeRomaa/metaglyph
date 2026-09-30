import * as Comlink from "comlink";
import { useStore } from "../state/store.ts";
import type { DragStep, EditResult, EngineResult, Op, Pt } from "./types.ts";
import type { EngineApi } from "./worker.ts";

function start() {
    const worker = new Worker(new URL("./worker.ts", import.meta.url), {
        type: "module",
    });
    return { worker, engine: Comlink.wrap<EngineApi>(worker) };
}

let current = start();

/** A failed engine call means a panic in the WebAssembly engine, which
 * leaves it unusable. Start a fresh worker and re-check the current text,
 * so the editor keeps working. */
function restart() {
    current.worker.terminate();
    current = start();
    const { text, version } = useStore.getState();
    pendingText = { source: text, version };
    useStore
        .getState()
        .setNotice("The engine failed and was restarted; see the console.");
}

let running = false;
let pendingText: { source: string; version: number } | null = null;
let pendingView = false;
type Remote = Comlink.Remote<EngineApi>;

/** Engine calls after the pending check, in order. */
const pendingCalls: {
    run: (engine: Remote) => Promise<unknown>;
    resolve: (result: unknown) => void;
    fallback: unknown;
}[] = [];
/** The newest drag move; a newer one replaces it (resolving it with
 * null), so a slow solve never builds a backlog. */
let pendingDrag: {
    at: Pt;
    resolve: (step: DragStep | null) => void;
} | null = null;
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

/** Queues an engine call behind any pending check, so the engine has seen
 * the current text first. `fallback` is the result if the call fails. */
function call<T>(run: (engine: Remote) => Promise<T>, fallback: T): Promise<T> {
    return new Promise((resolve) => {
        pendingCalls.push({
            run,
            resolve: resolve as (result: unknown) => void,
            fallback,
        });
        if (!running) void drain();
    });
}

/** Runs an edit op against document `version`. */
export function runEdit(op: Op, version: number): Promise<EditResult> {
    return call((e) => e.edit(op, version), {
        status: "invalid",
        message: "The edit failed.",
    } as EditResult);
}

/** Pins document `version` for an edit gesture (see `state/gesture.ts`). */
export function pin(version: number) {
    return call((e) => e.pin(version), false);
}

export function unpin() {
    return call((e) => e.unpin(), undefined);
}

export function drivers(instance: string, glyph: string, target: string) {
    return call((e) => e.drivers(instance, glyph, target), null);
}

export function dragBegin(
    instance: string,
    glyph: string,
    target: string,
    version: number,
) {
    return call((e) => e.dragBegin(instance, glyph, target, version), null);
}

/** The drag step for the pointer at `at`; null if a newer move replaced
 * this one before it ran. */
export function dragTo(at: Pt): Promise<DragStep | null> {
    return new Promise((resolve) => {
        pendingDrag?.resolve(null);
        pendingDrag = { at, resolve };
        if (!running) void drain();
    });
}

export function dragSet(driver: number, value: number) {
    return call((e) => e.dragSet(driver, value), null);
}

export function dragCycle(axis: number) {
    return call((e) => e.dragCycle(axis), null);
}

export function dragEnd(glyph: string) {
    return call((e) => e.dragEnd(glyph), undefined);
}

/** Builds every instance as TTF, `timestamp` seconds since the epoch. */
export function buildTtf(timestamp: number) {
    return call((e) => e.buildTtf(timestamp), null);
}

async function drain() {
    running = true;
    let failures = 0;
    while (
        pendingText ||
        pendingView ||
        pendingDrag ||
        pendingCalls.length > 0
    ) {
        if (pendingText || pendingView) {
            const text = pendingText;
            pendingText = null;
            pendingView = false;
            // Read the view arguments at send time, so they are never stale.
            const { instance, glyph } = useStore.getState();
            try {
                const result = text
                    ? await current.engine.update(
                          text.source,
                          text.version,
                          instance,
                          glyph,
                      )
                    : await current.engine.view(instance, glyph);
                listener?.(result);
                failures = 0;
            } catch (error) {
                console.error("mg engine failed", error);
                // Give up after repeated failures on the same text rather
                // than restarting forever.
                if (++failures <= 2) restart();
            }
            continue;
        }
        // Calls in order; a drag move goes after any calls queued before
        // it (its drag's begin, say).
        const job = pendingCalls.shift();
        if (job) {
            try {
                job.resolve(await job.run(current.engine));
            } catch (error) {
                console.error("mg engine call failed", error);
                job.resolve(job.fallback);
                restart();
            }
            continue;
        }
        const move = pendingDrag;
        pendingDrag = null;
        if (!move) continue;
        try {
            move.resolve(await current.engine.dragTo(move.at[0], move.at[1]));
        } catch (error) {
            console.error("mg drag failed", error);
            move.resolve(null);
            restart();
        }
    }
    running = false;
}

// The view follows the active instance and glyph.
useStore.subscribe((s, prev) => {
    if (s.instance !== prev.instance || s.glyph !== prev.glyph) requestView();
});
