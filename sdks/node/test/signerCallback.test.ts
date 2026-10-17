import { clientOptions, createSigner } from "@test/helpers";
import {
  Client,
  XmtpError,
  type PublicIdentity,
  type Signature,
  type SignerKind,
} from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

async function bounded<T>(call: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      call,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Signer callback did not settle")),
          5000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

describe("malformed signer callback results", () => {
  it.each(["identity", "kind", "signature"] as const)(
    "settles a malformed %s result with a typed sanitized callback failure",
    async (method) => {
      const { signer } = createSigner();
      if (method === "identity")
        signer.identity = () =>
          Promise.resolve({
            kind: "ethereum",
            identifier: undefined,
          } as unknown as PublicIdentity);
      if (method === "kind")
        signer.kind = () =>
          Promise.resolve({
            kind: "scw",
            chainId: NaN,
          } as unknown as SignerKind);
      if (method === "signature")
        signer.sign = () =>
          Promise.resolve({
            kind: "scw",
            bytes: new Uint8Array(),
            address: undefined,
            chainId: 1n,
          } as unknown as Signature);
      const error: unknown = await bounded(
        Client.create(
          signer,
          clientOptions({ storage: { location: "inMemory" } }),
        ),
      ).catch((reason: unknown) => reason);
      expect(error).toBeInstanceOf(XmtpError.Signer);
      if (!(error instanceof XmtpError.Signer))
        throw new Error("Expected a typed signer callback failure");
      expect(error.details.code === "SignerFailed").toBe(true);
      expect(error).toMatchObject({
        details: {
          code: "SignerFailed",
          category: "callback",
          retryable: false,
        },
      });
      expect((error as Error).message).toBe("signer callback failed");
      expect(error).not.toHaveProperty("cause");
    },
  );
});
