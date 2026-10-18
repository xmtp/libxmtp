import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { test } from "node:test";

import { bytesToHex, hexToBytes } from "./hex.ts";
import { sha256, utf8 } from "./sha256.ts";

const hex = (bytes: Uint8Array): string => bytesToHex(bytes);

test("matches the FIPS 180-4 examples", () => {
  assert.equal(
    hex(sha256(utf8(""))),
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
  assert.equal(
    hex(sha256(utf8("abc"))),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
  assert.equal(
    hex(
      sha256(utf8("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
    ),
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
  );
});

test("matches node:crypto across padding boundaries", () => {
  // 55, 56 and 64 bytes cross the one-block and two-block padding cases.
  for (const length of [0, 1, 55, 56, 63, 64, 65, 119, 120, 1000]) {
    const input = new Uint8Array(randomBytes(length));
    const expected = createHash("sha256").update(input).digest("hex");
    assert.equal(hex(sha256(input)), expected, `length ${String(length)}`);
  }
});

test("encodes text as UTF-8", () => {
  assert.equal(
    hex(sha256(utf8("Ç"))),
    createHash("sha256").update("Ç", "utf8").digest("hex"),
  );
});

test("hex round trips and rejects malformed text", () => {
  const bytes = new Uint8Array([0, 1, 15, 16, 255]);
  assert.equal(bytesToHex(bytes), "00010f10ff");
  assert.deepEqual(hexToBytes("00010F10ff"), bytes);
  assert.equal(hexToBytes("abc"), undefined);
  assert.equal(hexToBytes("zz"), undefined);
  assert.deepEqual(hexToBytes(""), new Uint8Array());
});
