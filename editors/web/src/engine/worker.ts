import * as Comlink from "comlink";
import init, { Engine } from "../wasm/mg_web.js";
import type { DocState } from "./types.ts";

const ready = init().then(() => new Engine());

const api = {
    async update(source: string, version: number): Promise<DocState> {
        const engine = await ready;
        return engine.update(source, version) as DocState;
    },
};

export type EngineApi = typeof api;

Comlink.expose(api);
