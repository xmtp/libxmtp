import assert from "node:assert/strict";

import { Timestamp } from "../../../../target/sdk-conformance/typescript-napi/index.ts";

assert.equal(new Timestamp(-1n).date.getTime(), -1);
assert.equal(new Timestamp(-1_000_000n).date.getTime(), -1);
assert.equal(new Timestamp(-1_000_001n).date.getTime(), -2);
assert.equal(new Timestamp(999_999n).date.getTime(), 0);
assert.equal(new Timestamp(1_000_000n).date.getTime(), 1);
