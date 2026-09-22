import { buildClient, createSigner } from "@test/helpers";
import type {
  AuthConfiguration,
  LimitsConfiguration,
  RetentionConfiguration,
} from "@xmtp/wasm-bindings";
import { describe, expect, it } from "vitest";

import { Client } from "@/Client";
import { fetchServerConfiguration } from "@/utils/serverConfiguration";

const backendUrl = import.meta.env.XMTP_BACKEND_URL as string;

const authFields: readonly (keyof AuthConfiguration)[] = [
  "enabled",
  "keys",
  "audiences",
  "issuers",
  "requiredScopes",
];

const retentionFields: readonly (keyof RetentionConfiguration)[] = [
  "groupMessageSeconds",
  "welcomeSeconds",
  "keyPackageSeconds",
];

// Spec 006 §5.2 publishes twenty limits. Every one is a JavaScript number.
const limitFields: readonly (keyof LimitsConfiguration)[] = [
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
];

describe("ServerConfiguration", () => {
  it("should read every field of the server configuration snapshot", async () => {
    const { identifier } = createSigner();
    const client = await buildClient(identifier);
    const configuration = client.serverConfiguration();

    expect(Object.keys(configuration).sort()).toEqual(
      [
        "auth",
        "identifier",
        "limits",
        "minLibxmtpVersion",
        "mls",
        "retention",
        "serverVersion",
        "smartContractWalletChains",
      ].sort(),
    );

    expect(typeof configuration.identifier).toBe("string");
    expect(configuration.identifier).not.toBe("");
    expect(typeof configuration.serverVersion).toBe("string");
    expect(typeof configuration.minLibxmtpVersion).toBe("string");

    const { auth } = configuration;
    expect(Object.keys(auth).sort()).toEqual([...authFields].sort());
    expect(typeof auth.enabled).toBe("boolean");
    for (const key of auth.keys) {
      expect(Object.keys(key).sort()).toEqual(["alg", "kid"]);
      expect(typeof key.kid).toBe("string");
      expect(typeof key.alg).toBe("string");
    }
    for (const list of [auth.audiences, auth.issuers, auth.requiredScopes]) {
      expect(Array.isArray(list)).toBe(true);
      for (const entry of list) expect(typeof entry).toBe("string");
    }

    const { retention } = configuration;
    expect(Object.keys(retention).sort()).toEqual([...retentionFields].sort());
    for (const field of retentionFields) {
      // §7: uint64 reaches JavaScript as a number, never a bigint.
      expect(typeof retention[field]).toBe("number");
      expect(retention[field]).toBeGreaterThan(0);
    }

    const { limits } = configuration;
    expect(Object.keys(limits).sort()).toEqual([...limitFields].sort());
    for (const field of limitFields) {
      expect(typeof limits[field]).toBe("number");
      expect(limits[field]).toBeGreaterThan(0);
      expect(limits[field]).toBeLessThan(Number.MAX_SAFE_INTEGER);
    }

    const { mls } = configuration;
    expect(typeof mls.maxGroupMembers).toBe("number");
    expect(mls.maxGroupMembers).toBeGreaterThan(0);
    expect(typeof mls.maxInstallationsPerInbox).toBe("number");
    expect(mls.maxInstallationsPerInbox).toBeGreaterThan(0);
    // `commitLogEnabled` is optional: absent means the compiled default.
    expect(["boolean", "undefined"]).toContain(typeof mls.commitLogEnabled);

    const chains = configuration.smartContractWalletChains;
    expect(Array.isArray(chains)).toBe(true);
    for (const chain of chains) expect(typeof chain).toBe("string");
  });

  it("should fetch the server configuration without a client", async () => {
    const configuration = await fetchServerConfiguration(backendUrl);
    expect(configuration.identifier).not.toBe("");
    expect(typeof configuration.serverVersion).toBe("string");
    expect(typeof configuration.auth.enabled).toBe("boolean");
    expect(configuration.limits.maxEnvelopeBytes).toBeGreaterThan(0);

    const fromClient = await Client.fetchServerConfiguration({ backendUrl });
    expect(fromClient.identifier).toBe(configuration.identifier);
  });

  it("should refresh the server configuration without changing the snapshot", async () => {
    const { identifier } = createSigner();
    const client = await buildClient(identifier);
    const snapshot = client.serverConfiguration();

    const refreshed = await client.refreshServerConfiguration();
    expect(refreshed.identifier).toBe(snapshot.identifier);
    expect(client.serverConfiguration()).toEqual(snapshot);
  });
});
