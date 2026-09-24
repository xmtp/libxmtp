import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-generated/typescript-napi/index.ts";

await sdk.uniffiInitAsync();
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
assert.equal(sdk.MessageID.fromString("a".repeat(64)).toString().length, 64);
console.log("Node scenario 1: load, checksums, version passed");
