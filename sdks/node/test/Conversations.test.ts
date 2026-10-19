import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

describe("Conversations", () => {
  it("should lift HMAC keys and debug info cursors to bytes and bigints", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);
    const dm = await client1.conversations.createDm(client2.inboxId);
    // The generated lift must give key bytes as a Uint8Array, the epoch as a
    // bigint, and each debug info cursor as a bigint. Rust tests check the
    // values, not this conversion.
    for (const conversation of [group, dm]) {
      const { cursor } = await conversation.debugInfo();
      expect(cursor.length).toBeGreaterThan(0);
      for (const value of cursor) {
        expect(typeof value).toBe("bigint");
      }
    }
    const hmacKeys = await client1.conversations.hmacKeys();
    expect([...hmacKeys.keys()].sort()).toEqual([group.id, dm.id].sort());
    const values = [...hmacKeys.values()].flat();
    expect(values.length).toBeGreaterThan(0);
    for (const value of values) {
      expect(value.key).toBeInstanceOf(Uint8Array);
      expect(value.key.length).toBe(42);
      expect(typeof value.epoch).toBe("bigint");
    }
  });

  it("should get a group or DM by ID", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    const group = await client1.conversations.createGroup([client2.inboxId]);
    expect(group).toBeDefined();
    expect(group.id).toBeDefined();
    const foundGroup = await client1.conversations.getById(group.id);
    expect(foundGroup).toBeDefined();
    expect(foundGroup!.id).toBe(group.id);

    const dm = await client1.conversations.createDm(client2.inboxId);
    expect(dm).toBeDefined();
    expect(dm.id).toBeDefined();
    const foundDm = await client1.conversations.getById(dm.id);
    expect(foundDm).toBeDefined();
    expect(foundDm!.id).toBe(dm.id);

    // The generated lowering must carry each camelCase list option into the
    // Rust filter. Rust tests check the filters, not this conversion.
    const ids = async (
      options: Parameters<typeof client1.conversations.list>[0],
    ) => (await client1.conversations.list(options)).map((c) => c.id);
    expect(await ids({ kind: "group" })).toEqual([group.id]);
    expect(await ids({ kind: "dm" })).toEqual([dm.id]);
    expect(await ids({ createdAfter: group.createdAt })).toEqual([dm.id]);
    expect(await ids({ createdBefore: dm.createdAt })).toEqual([group.id]);
    expect(
      await ids({ createdAfter: group.createdAt, createdBefore: dm.createdAt }),
    ).toEqual([]);
    expect(await ids({ limit: 1, orderBy: "createdAt" })).toEqual([dm.id]);
    await group.sendText("latest activity");
    expect(await ids({ limit: 1, orderBy: "lastActivity" })).toEqual([
      group.id,
    ]);
  });
});
