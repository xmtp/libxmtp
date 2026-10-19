import { Client, LocalSigningKey, type Signer } from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";

import { create } from "./helpers";

// Rust tests cover key parsing, export, and signing
// (crates/xmtp_sdk/src/signer/local_tests.rs). This case checks the generated
// class through the package: it is a Signer, its exports are async, it does
// not print its key, and a key read from a byte view creates a client.
describe("LocalSigningKey", () => {
  it("generates an exportable signer accepted directly by client creation", async () => {
    const key = await Client.generateKeys();
    const other = await Client.generateKeys();
    const hex = key.toHex();
    expect(hex).toBeInstanceOf(Promise);
    expect(await hex).not.toBe(await other.toHex());
    // The generated class must not expose the private key through string or
    // JSON conversion.
    const secret = (await key.toHex()).slice(2);
    expect(String(key)).not.toContain(secret);
    expect(JSON.stringify(key)).not.toContain(secret);
    const bytes = key.toBytes();
    expect(bytes).toBeInstanceOf(Promise);
    const backing = new Uint8Array(34);
    backing.set(await bytes, 1);
    const restored = await LocalSigningKey.fromBytes(backing.subarray(1, 33));
    const owner: Signer = restored;
    const client = await create(owner);
    expect(await client.isRegistered()).toBe(true);
    expect(await owner.identity()).toEqual(await key.identity());
    await client.end();
    expect(await key.toHex()).toBe(await restored.toHex());
  });
});
