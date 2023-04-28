export * from "./index_core.js";
import wasm from "./pkg/bindings_wasm_bg.wasm";
import { setWasmInit } from "./bindings_wasm.js";

// @ts-ignore
setWasmInit(() => wasm());
