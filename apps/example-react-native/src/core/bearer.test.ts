import assert from "node:assert/strict";
import { test } from "node:test";

import {
  NEVER_EXPIRES_SECONDS,
  bearerCredential,
  bearerHeader,
} from "./bearer.ts";
import { SessionCopy, SessionError } from "./sessionError.ts";

test("adds Bearer to a bare token (sessionCredentialUsesCompleteBearerHeader)", () => {
  assert.deepEqual(bearerCredential("fixture-access-token"), {
    value: "Bearer fixture-access-token",
    expiresAtSeconds: NEVER_EXPIRES_SECONDS,
  });
  assert.equal(NEVER_EXPIRES_SECONDS, 2n ** 63n - 1n);
});

test("keeps a typed Bearer prefix in any case and trims", () => {
  assert.equal(
    bearerHeader(" \nbEaReR fixture-access-token \t"),
    "bEaReR fixture-access-token",
  );
  assert.equal(bearerHeader("Bearer x"), "Bearer x");
  // "Bearer" without a space is part of the token.
  assert.equal(bearerHeader("Bearerx"), "Bearer Bearerx");
});

test("an empty token asks for one (emptyBearerTokenIsRejected)", () => {
  for (const token of ["", " \n\t"]) {
    assert.throws(
      () => bearerHeader(token),
      (error: unknown) =>
        error instanceof SessionError &&
        error.message === SessionCopy.enterToken,
    );
  }
});
