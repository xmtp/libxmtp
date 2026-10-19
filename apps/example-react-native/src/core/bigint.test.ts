import assert from "node:assert/strict";
import { test } from "node:test";

import { INT64_MAX, bigintReplacer, decimalToBigint } from "./bigint.ts";

test("bigints persist as decimal strings, and only plain decimal text reads back", () => {
  assert.equal(
    JSON.stringify({ ns: INT64_MAX, n: 1 }, bigintReplacer),
    '{"ns":"9223372036854775807","n":1}',
  );
  for (const value of [0n, 1n, -1n, INT64_MAX, 1_760_000_000_000_000_000n]) {
    const text = JSON.stringify({ value }, bigintReplacer);
    const stored: unknown = JSON.parse(text).value;
    assert.equal(typeof stored, "string", text);
    assert.equal(decimalToBigint(String(stored)), value, text);
  }
  for (const text of ["", "+1", "1.0", "1e3", " 1", "0x10", "abc", "-"]) {
    assert.equal(decimalToBigint(text), undefined, JSON.stringify(text));
  }
});
