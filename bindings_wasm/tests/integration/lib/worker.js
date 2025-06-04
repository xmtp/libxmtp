console.log("INIT SCRIPT WORKER");
import { default as init } from "/worker/dist/bindings_wasm_integration_worker.js";

async function init_wasm() {
  console.log("LOADING WASM");
  const wasm = await init(
    "/worker/dist/bindings_wasm_integration_worker_bg.wasm",
  );
}

await init_wasm();
