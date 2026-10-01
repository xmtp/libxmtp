import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { loggingContract } from "./logging-contract.js";
import { loggingEnd, loggingSecrets } from "./node-logging.mts";

const deadline = setTimeout(() => {
  throw new Error("Node logging conformance timed out");
}, 20_000);

await sdk.initLogging({
  level: "error",
  structured: true,
  performance: false,
  otel: undefined,
  resourceAttributes: new Map(),
});
let delivered!: () => void;
const seen = new Promise<void>((resolve) => {
  delivered = resolve;
});
await sdk.setLogSink({
  async log(record) {
    if (record.target !== "xmtp_sdk::conformance") return;
    delivered();
    assert.equal(sdk.sdkConformanceReadLock(), 1);
  },
});
await sdk.sdkConformanceEmitUnderLock();
await seen;
await sdk.setLogSink(undefined);

await loggingSecrets();
await loggingContract({
  setLogSink: sdk.setLogSink,
  emit: sdk.sdkConformanceEmit,
});
console.log(
  "Node isolated async queue, failure, generations, reentry and secrets passed",
);

const client = await sdk.Client.create(await sdk.generateLocalSigner(), {
  backend: { url: process.env.XMTP_BACKEND_URL! },
  storage: { location: "inMemory" },
  deviceSync: false,
  registration: { auto: false },
});
await loggingEnd(client);
console.log("Node client end from log callback passed");

clearTimeout(deadline);
