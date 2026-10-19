import assert from "node:assert/strict";
import { test } from "node:test";

import { bearerCredential } from "./bearer.ts";
import { SessionCopy, SessionError } from "./sessionError.ts";

test("the credential adds Bearer once, in any case, trims, and never expires", () => {
  assert.deepEqual(bearerCredential("fixture-access-token"), {
    value: "Bearer fixture-access-token",
    // Int64.max: the SDK takes signed 64-bit seconds.
    expiresAtSeconds: 2n ** 63n - 1n,
  });
  for (const [token, header] of [
    [" \nbEaReR fixture-access-token \t", "bEaReR fixture-access-token"],
    ["Bearer x", "Bearer x"],
    // "Bearer" without a space is part of the token.
    ["Bearerx", "Bearer Bearerx"],
  ] as const) {
    assert.equal(bearerCredential(token).value, header, token);
  }
});

test("an empty token asks for one", () => {
  for (const token of ["", " \n\t"]) {
    assert.throws(
      () => bearerCredential(token),
      (error: unknown) =>
        error instanceof SessionError &&
        error.message === SessionCopy.enterToken,
    );
  }
});
