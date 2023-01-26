export * from "./index_core.js";
import wasm from "./pkg/libxmtp_bg.wasm";
import { setWasmInit } from "./libxmtp.js";

// @ts-ignore
setWasmInit(() => wasm());
