import assert from "node:assert/strict";
import { test } from "node:test";

import { bytesToHex, hexToBytes } from "./hex.ts";

test("hex round trips every byte value and rejects malformed text", () => {
  const bytes = Uint8Array.from({ length: 256 }, (_, index) => index);
  const text = bytesToHex(bytes);
  assert.equal(text.slice(0, 8), "00010203");
  assert.equal(text.length, 512);
  assert.deepEqual(hexToBytes(text), bytes);
  assert.deepEqual(hexToBytes(text.toUpperCase()), bytes);
  assert.deepEqual(hexToBytes(""), new Uint8Array());
  for (const malformed of ["abc", "zz", "0x10", " 00"]) {
    assert.equal(hexToBytes(malformed), undefined, malformed);
  }
});
