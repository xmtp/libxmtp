import { uniffiInitAsync } from "../../../target/sdk-generated/typescript-wasm/index.ts";
import {
  initPureWasm,
  sdkVersion,
} from "../../../target/sdk-generated/typescript-pure/index.ts";

const wasm = new URL(
  "../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
await uniffiInitAsync(wasm);
await initPureWasm();
if (!sdkVersion().startsWith("1.12.")) throw new Error("pure WASM did not initialize");
console.log("XMTP SDK worker and pure WASM initialized");
