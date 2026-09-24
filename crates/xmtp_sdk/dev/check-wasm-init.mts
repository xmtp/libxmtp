import { uniffiInitAsync } from "../../../target/sdk-generated/typescript-wasm/index.ts";

const wasm = new URL(
  "../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
await uniffiInitAsync(wasm);
console.log("XMTP SDK WASM initialized");
