export * from "./index_core.js";
import wasm from "./pkg/wasmpkg_bg.wasm";
import { setWasmInit } from "./wasmpkg.js";

// @ts-ignore
setWasmInit(() => wasm());
