import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

await sdk.uniffiInitAsync();
await sdk.initLogging({
  level: sdk.LogLevel.Error,
  structured: true,
  performance: false,
  otel: undefined,
  resourceAttributes: new Map(),
});
let called = false;
sdk.setLogSink({
  log(record) {
    if (record.target !== "xmtp_sdk::conformance") return;
    called = true;
    assert.equal(sdk.sdkConformanceReadLock(), 1);
  },
});
await sdk.sdkConformanceEmitUnderLock();
await new Promise((resolve) => setTimeout(resolve, 100));
assert.ok(called, "the queued sink did not run");
sdk.setLogSink(undefined);
