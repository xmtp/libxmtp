import process from "node:process";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { Client, XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

const backendUrl = () => process.env.XMTP_BACKEND_URL!;

/** Public integer fields retain their generated width. */
const expectCount = (value: unknown) => {
  expect(["number", "bigint"]).toContain(typeof value);
  if (typeof value === "number") expect(Number.isInteger(value)).toBe(true);
  expect(value).toBeGreaterThanOrEqual(0);
};

describe("server configuration", () => {
  it("should read every field of the snapshot the client built with", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const configuration = client.serverConfiguration;

    // Top level.
    expect(typeof configuration.identifier).toBe("string");
    // A real published identifier, not the empty default an offline build
    // would hold: 1 to 256 bytes with no whitespace (spec 006 §4.1).
    expect(configuration.identifier).toMatch(/^\S+$/);
    expect(Buffer.byteLength(configuration.identifier)).toBeLessThanOrEqual(
      256,
    );
    expect(typeof configuration.serverVersion).toBe("string");
    expect(configuration.serverVersion.length).toBeGreaterThan(0);
    expect(typeof configuration.minLibxmtpVersion).toBe("string");
    expect(Array.isArray(configuration.smartContractWalletChains)).toBe(true);
    for (const chain of configuration.smartContractWalletChains) {
      expect(chain).toMatch(/^[-a-z0-9]+:[-_a-zA-Z0-9]+$/);
    }

    // Auth summary. The keys and the three lists exist whether auth is on or
    // off; off means every one of them is empty.
    const { auth } = configuration;
    expect(typeof auth.enabled).toBe("boolean");
    expect(Array.isArray(auth.keys)).toBe(true);
    for (const key of auth.keys) {
      expect(typeof key.kid).toBe("string");
      expect(typeof key.alg).toBe("string");
    }
    expect(Array.isArray(auth.audiences)).toBe(true);
    expect(Array.isArray(auth.issuers)).toBe(true);
    expect(Array.isArray(auth.requiredScopes)).toBe(true);
    if (!auth.enabled) {
      expect(auth.keys).toEqual([]);
      expect(auth.audiences).toEqual([]);
      expect(auth.issuers).toEqual([]);
      expect(auth.requiredScopes).toEqual([]);
    }

    // Retention, in seconds.
    const { retention } = configuration;
    expectCount(retention.groupMessageSeconds);
    expectCount(retention.welcomeSeconds);
    expectCount(retention.keyPackageSeconds);

    // Limits. The backend fills every one of them, so none is zero.
    const { limits } = configuration;
    const limitNames = [
      "maxEnvelopeBytes",
      "maxRequestBytes",
      "maxResponseBytes",
      "maxPublishTopics",
      "maxQueryTopics",
      "maxQueryLimit",
      "defaultQueryLimit",
      "maxNewestMetadataTopics",
      "maxNewestFullTopics",
      "maxUpdateAdds",
      "maxUpdateRemoves",
      "maxStreamTopics",
      "maxStaticTopics",
      "maxLookupIdentifiers",
      "maxScwSignatures",
      "maxIdentityEntries",
      "maxUpdateFramesPerSecond",
      "maxUpdateBurst",
      "maxPingFramesPerSecond",
      "maxPingBurst",
    ] as const;
    expect(Object.keys(limits).sort()).toEqual([...limitNames].sort());
    for (const name of limitNames) {
      expectCount(limits[name]);
      expect(limits[name]).toBeGreaterThan(0);
    }
    // The fixed 25 MiB transport ceiling.
    expect(limits.maxRequestBytes).toBeLessThanOrEqual(25 * 1024 * 1024);
    expect(limits.maxResponseBytes).toBeLessThanOrEqual(25 * 1024 * 1024);

    // MLS policy. `commitLogEnabled` is the one optional field: absent is
    // distinct from false.
    const { mls } = configuration;
    expectCount(mls.maxGroupMembers);
    expect(mls.maxGroupMembers).toBeGreaterThan(0);
    expectCount(mls.maxInstallationsPerInbox);
    expect(mls.maxInstallationsPerInbox).toBeGreaterThan(0);
    expect(["boolean", "undefined"]).toContain(typeof mls.commitLogEnabled);

    await client.end();
  });

  it("should fetch the configuration with no client and no database", async () => {
    const fetched = await Client.fetchServerConfiguration({
      url: backendUrl(),
    });

    expect(fetched.identifier.length).toBeGreaterThan(0);
    expect(fetched.serverVersion.length).toBeGreaterThan(0);
    expect(typeof fetched.auth.enabled).toBe("boolean");
    expect(Array.isArray(fetched.auth.requiredScopes)).toBe(true);
    expect(Array.isArray(fetched.smartContractWalletChains)).toBe(true);
    expect(fetched.limits.maxRequestBytes).toBeGreaterThan(0);

    // Network options and a bare URL reach the same deployment.
    const viaOptions = await Client.fetchServerConfiguration({
      url: backendUrl(),
      appVersion: "test/1.0.0",
    });
    expect(viaOptions).toEqual(fetched);

    // And it agrees with what a built client is holding.
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    expect(client.serverConfiguration).toEqual(fetched);
    await client.end();
  });

  it("should require a backend URL to fetch", async () => {
    await expect(
      Client.fetchServerConfiguration({ url: "  " }),
    ).rejects.toThrow("relative URL without a base");
  });

  it("should refresh without changing the snapshot the client holds", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const snapshot = client.serverConfiguration;

    const refreshed = await client.refreshServerConfiguration();
    expect(refreshed).toEqual(snapshot);
    // The snapshot is read once at build and never replaced.
    expect(client.serverConfiguration).toEqual(snapshot);

    await client.end();
  });

  // verifies: CONF-064
  it("exposes each configuration failure as a distinct public type", () => {
    const cases = [
      ["ConfigurationUnavailable", XmtpError.ConfigurationUnavailable],
      ["ConfigurationInvalid", XmtpError.ConfigurationInvalid],
      ["BackendMismatch", XmtpError.BackendMismatch],
      ["ClientVersionTooOld", XmtpError.ClientVersionTooOld],
      ["AuthRequired", XmtpError.AuthRequired],
      ["ChainNotAccepted", XmtpError.ChainNotAccepted],
    ] as const;
    for (const [code, Type] of cases) {
      const error = new Type({
        code,
        category: "configuration",
        retryable: false,
        message: "configuration failure",
      });
      expect(error).toBeInstanceOf(XmtpError);
      expect(error.details.code).toBe(code);
      for (const [otherCode, Other] of cases)
        if (otherCode !== code) expect(error).not.toBeInstanceOf(Other);
    }
  });
});
