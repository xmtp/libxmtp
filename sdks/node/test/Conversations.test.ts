import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

describe("Conversations", () => {
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
