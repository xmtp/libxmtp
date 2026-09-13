import { describe, expect, it } from "vitest";
import { createRegisteredClient, createSigner } from "@test/helpers";

describe("DebugInformation", () => {
  it("should return network API statistics", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);

    const apiStats = client.debugInformation.apiStatistics();
    // Registration publishes identity updates and a key package.
    expect(apiStats.publish).toBeGreaterThanOrEqual(2n);
    expect(apiStats.query).toBeGreaterThanOrEqual(2n);
    expect(apiStats.queryNewest).toBe(0n);
    expect(apiStats.subscribe).toBe(0n);
    expect(apiStats.subscribeStatic).toBe(0n);

    const apiIdentityStats = client.debugInformation.apiIdentityStatistics();
    expect(apiIdentityStats.getInboxIds).toBeGreaterThanOrEqual(1n);
    expect(apiIdentityStats.verifySmartContractWalletSignatures).toBe(0n);

    client.debugInformation.clearAllStatistics();

    const apiStats2 = client.debugInformation.apiStatistics();
    expect(apiStats2.publish).toBe(0n);
    expect(apiStats2.query).toBe(0n);
    expect(apiStats2.queryNewest).toBe(0n);
    expect(apiStats2.subscribe).toBe(0n);
    expect(apiStats2.subscribeStatic).toBe(0n);

    const apiIdentityStats2 = client.debugInformation.apiIdentityStatistics();
    expect(apiIdentityStats2.getInboxIds).toBe(0n);
    expect(apiIdentityStats2.verifySmartContractWalletSignatures).toBe(0n);

    const apiAggregateStats = client.debugInformation.apiAggregateStatistics();
    expect(apiAggregateStats).toBeDefined();
  });
});
