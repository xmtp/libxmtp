import {
  createRegisteredClient,
  createUser,
  TEST_API_URL,
} from "@test/helpers";
import { describe, expect, it } from "vitest";

import { fetchServerConfiguration, type ServerConfiguration } from "../dist";

/**
 * Read every field of the published configuration, so a field that
 * stops round-tripping fails here rather than in an app.
 */
const expectEveryField = (configuration: ServerConfiguration) => {
  expect(typeof configuration.identifier).toBe("string");
  expect(configuration.identifier.length).toBeGreaterThan(0);
  expect(typeof configuration.serverVersion).toBe("string");
  expect(typeof configuration.minLibxmtpVersion).toBe("string");

  const auth = configuration.auth;
  expect(typeof auth.enabled).toBe("boolean");
  expect(Array.isArray(auth.keys)).toBe(true);
  for (const key of auth.keys) {
    expect(typeof key.kid).toBe("string");
    expect(typeof key.alg).toBe("string");
  }
  expect(Array.isArray(auth.audiences)).toBe(true);
  expect(Array.isArray(auth.issuers)).toBe(true);
  expect(Array.isArray(auth.requiredScopes)).toBe(true);

  // uint64 on the wire, `number` in JavaScript: every published value is
  // below 2^53 (spec 006 §7).
  const retention = configuration.retention;
  for (const seconds of [
    retention.groupMessageSeconds,
    retention.welcomeSeconds,
    retention.keyPackageSeconds,
  ]) {
    expect(typeof seconds).toBe("number");
    expect(Number.isSafeInteger(seconds)).toBe(true);
    expect(seconds).toBeGreaterThan(0);
  }

  const limits = configuration.limits;
  for (const value of [
    limits.maxEnvelopeBytes,
    limits.maxRequestBytes,
    limits.maxResponseBytes,
    limits.maxPublishTopics,
    limits.maxQueryTopics,
    limits.maxQueryLimit,
    limits.defaultQueryLimit,
    limits.maxNewestMetadataTopics,
    limits.maxNewestFullTopics,
    limits.maxUpdateAdds,
    limits.maxUpdateRemoves,
    limits.maxStreamTopics,
    limits.maxStaticTopics,
    limits.maxLookupIdentifiers,
    limits.maxScwSignatures,
    limits.maxIdentityEntries,
    limits.maxUpdateFramesPerSecond,
    limits.maxUpdateBurst,
    limits.maxPingFramesPerSecond,
    limits.maxPingBurst,
  ]) {
    expect(typeof value).toBe("number");
    expect(Number.isSafeInteger(value)).toBe(true);
    // The backend fills every limit, so a client never reads zero.
    expect(value).toBeGreaterThan(0);
  }
  // Never above the fixed 25 MiB transport ceiling.
  expect(limits.maxRequestBytes).toBeLessThanOrEqual(25 * 1024 * 1024);
  expect(limits.maxResponseBytes).toBeLessThanOrEqual(25 * 1024 * 1024);

  const mls = configuration.mls;
  expect(typeof mls.maxGroupMembers).toBe("number");
  expect(mls.maxGroupMembers).toBeGreaterThan(0);
  expect(typeof mls.maxInstallationsPerInbox).toBe("number");
  expect(mls.maxInstallationsPerInbox).toBeGreaterThan(0);
  // `Option<bool>` is a nullable boolean: absent is distinct from `false`.
  expect(
    mls.commitLogEnabled === null ||
      mls.commitLogEnabled === undefined ||
      typeof mls.commitLogEnabled === "boolean",
  ).toBe(true);

  expect(Array.isArray(configuration.smartContractWalletChains)).toBe(true);
  for (const chain of configuration.smartContractWalletChains) {
    expect(chain).toMatch(/^[a-z0-9-]{1,8}:[A-Za-z0-9-_]{1,32}$/);
  }
};

describe("ServerConfiguration", () => {
  // verifies: CONF-061
  it("should read every field of the snapshot the client resolved at build", async () => {
    const client = await createRegisteredClient(createUser());
    try {
      const configuration = client.serverConfiguration();
      expectEveryField(configuration);
      expect(configuration.identifier).toBe("org.xmtp.local");
      expect(configuration.limits.maxQueryLimit).toBe(50);
      expect(configuration.limits.defaultQueryLimit).toBe(50);
      expect(configuration.smartContractWalletChains).toContain("eip155:31337");
    } finally {
      await client.close();
    }
  });

  // verifies: CONF-062
  it("should fetch the configuration without a client", async () => {
    const configuration = await fetchServerConfiguration(TEST_API_URL);
    expectEveryField(configuration);
    expect(configuration.identifier).toBe("org.xmtp.local");
  });

  // The app version is the only other transport argument, and it is
  // optional.
  it("should fetch the configuration with an app version", async () => {
    const configuration = await fetchServerConfiguration(
      TEST_API_URL,
      "test/1.0.0",
    );
    expect(configuration.identifier).toBe("org.xmtp.local");
  });

  // verifies: CONF-064
  it("should reject with ConfigurationUnavailable when the backend is unreachable", async () => {
    await expect(
      fetchServerConfiguration("http://127.0.0.1:1"),
    ).rejects.toThrow(/\[ClientError::ConfigurationUnavailable\]/);
  });

  // verifies: CONF-074
  it("should refresh and return the fetched configuration", async () => {
    const client = await createRegisteredClient(createUser());
    try {
      const snapshot = client.serverConfiguration();
      const refreshed = await client.refreshServerConfiguration();
      expectEveryField(refreshed);
      expect(refreshed).toEqual(snapshot);
      expect(client.serverConfiguration()).toEqual(snapshot);
    } finally {
      await client.close();
    }
  });
});
