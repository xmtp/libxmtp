import {
  Client,
  LocalSigningKey,
  XmtpError,
  type Signer,
} from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";

import { create } from "./helpers";

const fixed =
  "0x0000000000000000000000000000000000000000000000000000000000000001";

describe("LocalSigningKey", () => {
  // verifies: IDENT-088, IDENT-089
  it("restores a known account and keeps exported bytes independent", async () => {
    const key = await LocalSigningKey.fromHex(fixed);
    expect(await key.identity()).toEqual({
      kind: "ethereum",
      identifier: "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf",
    });
    const bytesResult = key.toBytes();
    expect(bytesResult).toBeInstanceOf(Promise);
    const bytes = await bytesResult;
    expect(bytes).toHaveLength(32);
    const backing = new Uint8Array(34);
    backing.set(bytes, 1);
    const restored = await LocalSigningKey.fromBytes(backing.subarray(1, 33));
    bytes.fill(0);
    const hexResult = key.toHex();
    expect(hexResult).toBeInstanceOf(Promise);
    expect(await hexResult).toBe(fixed);
    expect(await restored.toHex()).toBe(fixed);
    expect(await restored.sign({ text: "restore account" })).toEqual(
      await key.sign({ text: "restore account" }),
    );
    expect(String(key)).not.toContain(fixed.slice(2));
    expect(JSON.stringify(key)).not.toContain(fixed.slice(2));
  });

  it("rejects invalid private keys with public input errors", async () => {
    for (const bytes of [
      new Uint8Array(31),
      new Uint8Array(33),
      new Uint8Array(32),
      new Uint8Array(32).fill(255),
    ]) {
      await expect(LocalSigningKey.fromBytes(bytes)).rejects.toBeInstanceOf(
        XmtpError.InvalidInput,
      );
    }
    for (const hex of [fixed.slice(2), fixed + "00", "0x" + "g".repeat(64)]) {
      await expect(LocalSigningKey.fromHex(hex)).rejects.toBeInstanceOf(
        XmtpError.InvalidInput,
      );
    }
  });

  it("generates an exportable signer accepted directly by client creation", async () => {
    const key = await Client.generateKeys();
    const other = await Client.generateKeys();
    expect(await key.toHex()).not.toBe(await other.toHex());
    const restored = await LocalSigningKey.fromHex(await key.toHex());
    const owner: Signer = restored;
    const client = await create(owner);
    expect(await client.isRegistered()).toBe(true);
    expect(await owner.identity()).toEqual(await key.identity());
    await client.end();
    expect(await key.toHex()).toBe(await restored.toHex());
  });
});
