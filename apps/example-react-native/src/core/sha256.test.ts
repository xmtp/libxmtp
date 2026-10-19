import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import { bytesToHex } from "./hex.ts";
import { sha256, utf8 } from "./sha256.ts";

test("matches node:crypto across padding boundaries and for UTF-8 text", () => {
  // 55, 56 and 64 bytes cross the one-block and two-block padding cases.
  for (const length of [0, 1, 3, 55, 56, 63, 64, 65, 119, 120, 1000]) {
    const input = Uint8Array.from(
      { length },
      (_, index) => (index * 31 + length) % 256,
    );
    const expected = createHash("sha256").update(input).digest("hex");
    assert.equal(
      bytesToHex(sha256(input)),
      expected,
      `length ${String(length)}`,
    );
  }
  for (const text of ["", "abc", "Ç", "グループ 🎉"]) {
    assert.equal(
      bytesToHex(sha256(utf8(text))),
      createHash("sha256").update(text, "utf8").digest("hex"),
      text,
    );
  }
});
