import { createRegisteredClient, createSigner } from "@test/helpers";
import { Group, XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

describe("LibXMTP errors", () => {
  it("should throw when a non-admin tries to add members", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();

    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);

    // Add client2 as a member of an admin-only group.
    const group = await client1.conversations.createGroup([client2.inboxId], {
      permissions: { kind: "adminOnly" },
    });

    // Sync client2 before the permission check.
    await client2.conversations.sync();
    const group2 = await client2.conversations.getById(group.id);

    // A member cannot add client3.
    if (!(group2 instanceof Group)) {
      throw new Error("Expected a Group conversation");
    }

    await expect(group2.addMembers([client3.inboxId])).rejects.toThrow(
      "Insufficient permissions",
    );
  });

  it("should throw when adding a non-existent inbox ID", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);

    const group = await client.conversations.createGroup([]);

    const fakeInboxId =
      "0000000000000000000000000000000000000000000000000000000000000000";

    try {
      await group.addMembers([fakeInboxId]);
      expect.fail("Expected an error to be thrown");
    } catch (error) {
      expect(error).toBeInstanceOf(XmtpError);
      expect((error as Error).message).toContain("SequenceId not found");
    }
  });
});
