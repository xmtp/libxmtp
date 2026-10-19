import { createRegisteredClient, endAfterTest } from "@test/helpers";
import { Client, LocalSigningKey, type Signer } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

// Rust tests cover key parsing, export, and signing
// (crates/xmtp_sdk/src/signer/local_tests.rs). This case checks the generated
// class through the package: it is a Signer, its exports are async, and a key
// read from a byte view creates a client.
describe("LocalSigningKey", () => {
  it("generates an exportable signer accepted directly by client creation", async () => {
    const key = await Client.generateKeys();
    const other = await Client.generateKeys();
    const hex = key.toHex();
    expect(hex).toBeInstanceOf(Promise);
    expect(await hex).not.toBe(await other.toHex());
    const bytes = key.toBytes();
    expect(bytes).toBeInstanceOf(Promise);
    const backing = new Uint8Array(34);
    backing.set(await bytes, 1);
    const restored = await LocalSigningKey.fromBytes(backing.subarray(1, 33));
    const owner: Signer = restored;
    const client = endAfterTest(await createRegisteredClient(owner));
    expect(await client.isRegistered()).toBe(true);
    expect(await owner.identity()).toEqual(await key.identity());
    await client.end();
    expect(await key.toHex()).toBe(await restored.toHex());
  });
});
