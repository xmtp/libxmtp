import process from "node:process";
import { describe, expect, it } from "vitest";
import { Client } from "@/Client";
import {
  AuthRequiredError,
  BackendMismatchError,
  ChainNotAcceptedError,
  ClientVersionTooOldError,
  ConfigurationInvalidError,
  ConfigurationUnavailableError,
  ServerConfigurationError,
  toServerConfigurationError,
} from "@/ServerConfiguration";
import { createRegisteredClient, createSigner } from "@test/helpers";

const backendUrl = () => process.env.XMTP_BACKEND_URL!;

/** Every `uint64` and `uint32` of spec 006 §5.2 is a JavaScript `number`. */
const expectCount = (value: unknown) => {
  expect(typeof value).toBe("number");
  expect(Number.isInteger(value)).toBe(true);
  expect(value as number).toBeGreaterThanOrEqual(0);
};

describe("server configuration", () => {
  it("should read every field of the snapshot the client built with", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const configuration = client.serverConfiguration();

    // Top level (CFG-080).
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

    // Limits. The backend fills every one of them (CFG-026), so none is zero.
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
    // The fixed 25 MiB transport ceiling of CFG-007 and CFG-071.
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

    await client.close();
  });

  it("should fetch the configuration with no client and no database", async () => {
    const fetched = await Client.fetchServerConfiguration(backendUrl());

    expect(fetched.identifier.length).toBeGreaterThan(0);
    expect(fetched.serverVersion.length).toBeGreaterThan(0);
    expect(typeof fetched.auth.enabled).toBe("boolean");
    expect(Array.isArray(fetched.auth.requiredScopes)).toBe(true);
    expect(Array.isArray(fetched.smartContractWalletChains)).toBe(true);
    expect(fetched.limits.maxRequestBytes).toBeGreaterThan(0);

    // Network options and a bare URL reach the same deployment.
    const viaOptions = await Client.fetchServerConfiguration({
      backendUrl: backendUrl(),
      appVersion: "test/1.0.0",
    });
    expect(viaOptions).toEqual(fetched);

    // And it agrees with what a built client is holding.
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    expect(client.serverConfiguration()).toEqual(fetched);
    await client.close();
  });

  it("should require a backend URL to fetch", async () => {
    await expect(Client.fetchServerConfiguration("  ")).rejects.toThrow(
      "backendUrl is required",
    );
  });

  it("should refresh without changing the snapshot the client holds", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const snapshot = client.serverConfiguration();

    const refreshed = await client.refreshServerConfiguration();
    expect(refreshed).toEqual(snapshot);
    // The snapshot is read once at build and never replaced (CFG-030).
    expect(client.serverConfiguration()).toEqual(snapshot);

    await client.close();
  });

  it("should surface each configuration failure as its own type", () => {
    const cases = [
      ["ClientError::ConfigurationUnavailable", ConfigurationUnavailableError],
      ["ClientError::ConfigurationInvalid", ConfigurationInvalidError],
      ["ClientError::BackendMismatch", BackendMismatchError],
      ["ClientError::ClientVersionTooOld", ClientVersionTooOldError],
      ["ClientError::AuthRequired", AuthRequiredError],
      ["ClientError::ChainNotAccepted", ChainNotAcceptedError],
    ] as const;

    for (const [code, type] of cases) {
      const binding = new Error(`[${code}] something went wrong`);
      const typed = toServerConfigurationError(binding);
      expect(typed).toBeInstanceOf(type);
      expect(typed).toBeInstanceOf(ServerConfigurationError);
      expect(typed?.code).toBe(code);
      expect(typed?.message).toBe("something went wrong");
      expect(typed?.cause).toBe(binding);
      // Each class is distinct: no other case matches it.
      for (const [otherCode, otherType] of cases) {
        if (otherCode !== code) expect(typed).not.toBeInstanceOf(otherType);
      }
    }

    // Anything else is left alone.
    expect(
      toServerConfigurationError(new Error("[GroupError::Sync] nope")),
    ).toBeUndefined();
    expect(toServerConfigurationError("not an error")).toBeUndefined();
  });
});
