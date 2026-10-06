import { Client, XmtpError, type Signature } from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

import { options, signer } from "./helpers";

const SECRET = "private signer detail 0x5ec2e7";

// The signer runs on the main thread while Rust waits in the worker. A
// failure crosses the worker as a typed error without the app's text.
test.each([
  [
    "throws an Error",
    () => {
      throw new Error(SECRET);
    },
  ],
  [
    "rejects with a plain object",
    () =>
      // oxlint-disable-next-line typescript/prefer-promise-reject-errors -- Check a non-Error signer rejection.
      Promise.reject({ message: SECRET, toString: () => SECRET }),
  ],
  [
    "returns a malformed signature",
    () =>
      Promise.resolve({
        kind: "scw",
        bytes: new Uint8Array(),
        address: undefined,
        chainId: 1n,
      } as unknown as Signature),
  ],
] as const)(
  "a signer that %s fails create with a sanitized typed error",
  async (_label, sign) => {
    const owner = signer();
    owner.sign = sign;
    const error: unknown = await Client.create(owner, options).then(
      async (client) => {
        await client.end();
        throw new Error("create succeeded with a failing signer");
      },
      (reason: unknown) => reason,
    );
    expect(error).toBeInstanceOf(XmtpError.Signer);
    expect(error).toMatchObject({
      message: "signer callback failed",
      details: { code: "SignerFailed", category: "callback", retryable: false },
    });
    expect(error).not.toHaveProperty("cause");
    expect(JSON.stringify((error as XmtpError).details)).not.toContain(SECRET);
    expect(String(error)).not.toContain(SECRET);
  },
);
