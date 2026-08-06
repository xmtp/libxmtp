import { describe, expect, it } from "vitest";
import { createRegisteredClient, createSigner } from "@test/helpers";

describe("DebugInformation", () => {
  it("should return network API statistics", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);

    const apiStats = await client.debugInformation.apiStatistics();
    expect(apiStats.fetchKeyPackage).toBe(0n);
    expect(apiStats.queryGroupMessages).toBe(0n);
    expect(apiStats.queryWelcomeMessages).toBe(0n);
    expect(apiStats.sendGroupMessages).toBe(0n);
    expect(apiStats.sendWelcomeMessages).toBe(0n);
    expect(apiStats.subscribeMessages).toBe(0n);
    expect(apiStats.subscribeWelcomes).toBe(0n);
    expect(apiStats.uploadKeyPackage).toBe(1n);

    const apiIdentityStats =
      await client.debugInformation.apiIdentityStatistics();
    // These reflect identity-API network calls made during registration. The
    // exact count varies with timing/retries (e.g. an extra getIdentityUpdatesV2
    // fetch under slower CI / grpc-web latency), so assert a lower bound rather
    // than an exact count to avoid flakiness.
    expect(apiIdentityStats.getIdentityUpdatesV2).toBeGreaterThanOrEqual(2n);
    expect(apiIdentityStats.getInboxIds).toBeGreaterThanOrEqual(1n);
    expect(apiIdentityStats.publishIdentityUpdate).toBeGreaterThanOrEqual(1n);
    expect(apiIdentityStats.verifySmartContractWalletSignature).toBe(0n);

    await client.debugInformation.clearAllStatistics();

    // Background workers (identity refresh, device-sync) can land a call
    // between clear and these reads (seen: getIdentityUpdatesV2 and
    // queryGroupMessages at 1n), so tolerate at most one per counter.
    const apiStats2 = await client.debugInformation.apiStatistics();
    expect(apiStats2.uploadKeyPackage).toBeLessThanOrEqual(1n);
    expect(apiStats2.fetchKeyPackage).toBeLessThanOrEqual(1n);
    expect(apiStats2.sendWelcomeMessages).toBeLessThanOrEqual(1n);
    expect(apiStats2.queryGroupMessages).toBeLessThanOrEqual(1n);
    expect(apiStats2.queryWelcomeMessages).toBeLessThanOrEqual(1n);
    expect(apiStats2.subscribeMessages).toBeLessThanOrEqual(1n);

    const apiIdentityStats2 =
      await client.debugInformation.apiIdentityStatistics();
    // Pre-clear this was >= 2n, so <= 1n still proves the clear happened.
    expect(apiIdentityStats2.getIdentityUpdatesV2).toBeLessThanOrEqual(1n);
    expect(apiIdentityStats2.getInboxIds).toBeLessThanOrEqual(1n);
    expect(apiIdentityStats2.publishIdentityUpdate).toBeLessThanOrEqual(1n);
    expect(apiIdentityStats2.verifySmartContractWalletSignature).toBe(0n);

    const apiAggregateStats =
      await client.debugInformation.apiAggregateStatistics();
    expect(apiAggregateStats).toBeDefined();
  });
});
