import { describe, expect, it } from "vitest";
import { createRegisteredClient, createSigner } from "@test/helpers";

describe("DebugInformation", () => {
  it("should return network API statistics", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);

    const apiStats = await client.debugInformation.apiStatistics();
    // Registration publishes identity updates and a key package.
    expect(apiStats.publish).toBeGreaterThanOrEqual(2n);
    expect(apiStats.query).toBeGreaterThanOrEqual(2n);
    expect(apiStats.queryNewest).toBe(0n);
    expect(apiStats.get).toBe(0n);
    expect(apiStats.subscribe).toBe(0n);
    expect(apiStats.subscribeStatic).toBe(0n);

    const apiIdentityStats =
      await client.debugInformation.apiIdentityStatistics();
    expect(apiIdentityStats.getInboxIds).toBeGreaterThanOrEqual(1n);
    expect(apiIdentityStats.verifySmartContractWalletSignatures).toBe(0n);

    await client.debugInformation.clearAllStatistics();

    const apiStats2 = await client.debugInformation.apiStatistics();
    expect(apiStats2.publish).toBeLessThanOrEqual(1n);
    expect(apiStats2.query).toBeLessThanOrEqual(1n);
    expect(apiStats2.queryNewest).toBeLessThanOrEqual(1n);
    expect(apiStats2.get).toBeLessThanOrEqual(1n);
    expect(apiStats2.subscribe).toBeLessThanOrEqual(1n);
    expect(apiStats2.subscribeStatic).toBeLessThanOrEqual(1n);

    const apiIdentityStats2 =
      await client.debugInformation.apiIdentityStatistics();
    expect(apiIdentityStats2.getInboxIds).toBeLessThanOrEqual(1n);
    expect(apiIdentityStats2.verifySmartContractWalletSignatures).toBe(0n);

    const apiAggregateStats =
      await client.debugInformation.apiAggregateStatistics();
    expect(apiAggregateStats).toBeDefined();
  });
});
