import assert from "node:assert/strict";
import { test } from "node:test";

import {
  INT64_MAX,
  INT64_MIN,
  bigintReplacer,
  bigintToDecimal,
  decimalToBigint,
  isInt64,
  nanosecondsToMilliseconds,
} from "./bigint.ts";

test("decimal strings round trip every Int64", () => {
  for (const value of [
    0n,
    1n,
    -1n,
    INT64_MAX,
    INT64_MIN,
    1_760_000_000_000_000_000n,
  ]) {
    assert.equal(decimalToBigint(bigintToDecimal(value)), value);
  }
});

test("only plain decimal text parses", () => {
  for (const text of ["", "+1", "1.0", "1e3", " 1", "0x10", "abc", "-"]) {
    assert.equal(decimalToBigint(text), undefined, JSON.stringify(text));
  }
});

test("JSON with bigints uses decimal strings", () => {
  assert.throws(() => JSON.stringify({ ns: 1n }));
  assert.equal(
    JSON.stringify({ ns: INT64_MAX, n: 1 }, bigintReplacer),
    '{"ns":"9223372036854775807","n":1}',
  );
});

test("Int64 bounds and nanoseconds to milliseconds", () => {
  assert.ok(isInt64(INT64_MAX));
  assert.ok(!isInt64(INT64_MAX + 1n));
  assert.ok(!isInt64(INT64_MIN - 1n));
  assert.equal(nanosecondsToMilliseconds(1_999_999n), 1);
});
