import { createRegisteredClient, createSigner } from "@test/helpers";
import type { Client, MessageId } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

describe("pending recovery pages", () => {
  it("lifts messages and passes both raw position bounds through the native binding", async () => {
    const client = await createRegisteredClient(createSigner().signer, {
      storage: { location: "inMemory" },
    });
    let peer: Client | undefined;
    try {
      peer = await createRegisteredClient(createSigner().signer, {
        storage: { location: "inMemory" },
      });
      for (const conversation of [
        await client.conversations.createGroup([]),
        await client.conversations.createDm(peer.inboxId),
      ]) {
        const ids: MessageId[] = [];
        for (const text of ["first", "second", "third"])
          ids.push(await conversation.sendText(text, { optimistic: true }));
        const first = await conversation.messageRecoveryPage({
          limit: 2,
          kind: "application",
        });
        expect(first.messages.map((message) => message.id)).toEqual(
          ids.slice(0, 2),
        );
        expect(first.messages.map((message) => message.content)).toEqual([
          { kind: "text", value: "first" },
          { kind: "text", value: "second" },
        ]);
        expect(first.skippedCount).toBe(0);
        expect(first.hasMore).toBe(true);
        expect(first.firstPosition?.sentAt).toEqual(first.messages[0]?.sentAt);
        expect(first.lastPosition?.sentAt).toEqual(first.messages[1]?.sentAt);
        expect(first.lastPosition?.messageCursor).toBeTypeOf("string");

        const next = await conversation.messageRecoveryPage(
          { limit: 2, kind: "application" },
          undefined,
          first.lastPosition,
        );
        expect(next.messages.map((message) => message.id)).toEqual(
          ids.slice(2),
        );
        expect(next.hasMore).toBe(false);
        const older = await conversation.messageRecoveryPage(
          { limit: 2, kind: "application", direction: "descending" },
          next.firstPosition,
        );
        expect(older.messages.map((message) => message.id)).toEqual(
          ids.slice(0, 2).reverse(),
        );
        await conversation.publishMessage(ids[1]!);
        const fresh = await conversation.sendText("after publication", {
          optimistic: true,
        });
        const resumed = await conversation.messageRecoveryPage(
          { kind: "application" },
          undefined,
          first.lastPosition,
        );
        expect(resumed.messages.map((message) => message.id)).toEqual([fresh]);
        const defaults = await conversation.messageRecoveryPage();
        expect(
          defaults.messages
            .filter((message) => message.id === fresh)
            .map((message) => message.id),
        ).toEqual([fresh]);
      }
    } finally {
      await client.end();
      await peer?.end();
    }
  });
});
