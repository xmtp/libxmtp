import { createSigner } from "@test/helpers";
import { Client, type Credential } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

async function bounded<T>(call: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      call,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Credential callback did not settle")),
          5000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

describe("credential validation", () => {
  it.each([NaN, Infinity, 0.5, Number.MAX_SAFE_INTEGER + 1, 1])(
    "rejects invalid numeric expiration %s through the callback boundary",
    async (expiresAtSeconds) => {
      const identity = createSigner().identifier;
      const error: unknown = await bounded(
        Client.canMessage([identity], {
          url: process.env.XMTP_BACKEND_URL!,
          credentials: {
            credential: () =>
              Promise.resolve({
                value: "Bearer private-credential-value",
                expiresAtSeconds: expiresAtSeconds as unknown as bigint,
              }),
          },
        }),
      ).catch((reason: unknown) => reason);
      expect(error).toMatchObject({
        details: {
          code: "CredentialCallbackFailed",
          category: "callback",
          retryable: true,
        },
      });
      expect(JSON.stringify(error)).not.toContain("private-credential-value");
      expect(error).not.toHaveProperty("cause");
    },
  );
  it.each([-9223372036854775809n, 9223372036854775808n])(
    "rejects an expiration outside signed i64 %s through the callback boundary",
    async (expiresAtSeconds) => {
      const error: unknown = await bounded(
        Client.canMessage([createSigner().identifier], {
          url: process.env.XMTP_BACKEND_URL!,
          credentials: {
            credential: () =>
              Promise.resolve({
                value: "Bearer private-credential-value",
                expiresAtSeconds,
              }),
          },
        }),
      ).catch((reason: unknown) => reason);
      expect(error).toMatchObject({
        details: {
          code: "CredentialCallbackFailed",
          category: "callback",
          retryable: true,
        },
      });
      expect(JSON.stringify(error)).not.toContain("private-credential-value");
      expect(error).not.toHaveProperty("cause");
    },
  );
  it("settles a missing credential value with typed callback failure", async () => {
    const malformed = { expiresAtSeconds: 9223372036854775807n } as Credential;
    const error: unknown = await bounded(
      Client.canMessage([createSigner().identifier], {
        url: process.env.XMTP_BACKEND_URL!,
        credentials: { credential: () => Promise.resolve(malformed) },
      }),
    ).catch((reason: unknown) => reason);
    expect(error).toMatchObject({
      details: {
        code: "CredentialCallbackFailed",
        category: "callback",
        retryable: true,
      },
    });
    expect(error).not.toHaveProperty("cause");
  });
  it.each([
    BigInt(Math.floor(Date.now() / 1000) + 3600),
    9007199254740992n,
    9223372036854775807n,
    -9223372036854775808n,
  ])(
    "accepts legitimate bigint expiration %s without numeric conversion",
    async (expiresAtSeconds) => {
      const identity = createSigner().identifier;
      let calls = 0;
      const value: Credential = {
        value: "Bearer synthetic-bigint-edge",
        expiresAtSeconds,
      };
      const result = await bounded(
        Client.canMessage([identity], {
          url: process.env.XMTP_BACKEND_URL!,
          credentials: {
            credential: () => {
              calls++;
              return Promise.resolve(value);
            },
          },
        }),
      );
      expect(result.get(`ethereum:${identity.identifier}`)).toBe(false);
      expect(calls).toBe(1);
    },
  );
  it("does not retain a callback error or its cause", async () => {
    const failure = new Error("private refresh response");
    const error: unknown = await bounded(
      Client.canMessage([createSigner().identifier], {
        url: process.env.XMTP_BACKEND_URL!,
        credentials: { credential: () => Promise.reject(failure) },
      }),
    ).catch((reason: unknown) => reason);
    expect(error).toMatchObject({
      details: {
        code: "CredentialCallbackFailed",
        category: "callback",
        retryable: true,
      },
    });
    expect((error as Error).message).not.toContain(failure.message);
    expect(error).not.toHaveProperty("cause");
  });
});
