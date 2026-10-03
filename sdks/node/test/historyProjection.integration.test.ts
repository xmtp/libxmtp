import { createRegisteredClient, createSigner } from "@test/helpers";
import { type Client, type Dm, type Group, TextCodec } from "@xmtp/node-sdk";
import { expect, it } from "vitest";

async function setup(scope: "group" | "dm" | "all") {
  const client = await createRegisteredClient(createSigner().signer);
  const peer =
    scope === "dm"
      ? await createRegisteredClient(createSigner().signer)
      : undefined;
  const conversation = peer
    ? await client.conversations.createDm(peer.inboxId)
    : await client.conversations.createGroup([]);
  return { client, peer, conversation };
}
async function snapshot(
  scope: "group" | "dm" | "all",
  client: Client,
  conversation: Group | Dm,
) {
  return scope === "all"
    ? client.conversations.messageHistorySnapshot(128)
    : conversation.messageHistorySnapshot(128);
}

it.each(["group", "dm", "all"] as const)(
  "keeps reply parents, reply count and reactions in the %s history snapshot",
  async (scope) => {
    const { client, peer, conversation } = await setup(scope);
    try {
      const parent = await conversation.sendText("snapshot parent");
      const reply = await conversation.sendReply(
        parent,
        client.inboxId,
        new TextCodec().encode("snapshot reply"),
      );
      const reaction = await conversation.sendReaction(parent, client.inboxId, {
        action: "added",
        schema: "unicode",
        content: "👍",
      });
      const history = await snapshot(scope, client, conversation);
      const original = history.messages.find(
        (message) => message.id === parent,
      );
      const replyMessage = history.messages.find(
        (message) => message.id === reply,
      );
      expect(original?.replyCount).toBe(1n);
      expect(original?.reactions).toHaveLength(1);
      expect(original?.reactions[0]?.id).toBe(reaction);
      expect(replyMessage?.inReplyTo?.id).toBe(parent);
      expect(original?.deliveryCursor).toBeDefined();
      expect(replyMessage?.deliveryCursor).toBeDefined();
      expect(typeof history.cursor).toBe("string");
    } finally {
      await client.end();
      await peer?.end();
    }
  },
);

it.each(["group", "dm", "all"] as const)(
  "keeps authorized deletion and removes original bytes in the %s history snapshot",
  async (scope) => {
    const { client, peer, conversation } = await setup(scope);
    try {
      const parent = await conversation.sendText("snapshot deleted secret");
      await conversation.deleteMessage(parent);
      const ordinary = await client.conversations.getMessageById(parent);
      expect(ordinary?.content.kind).toBe("deletedMessage");
      const history = await snapshot(scope, client, conversation);
      const deleted = history.messages.find((message) => message.id === parent);
      expect(deleted?.content.kind).toBe("deletedMessage");
      expect(deleted?.rawBytes.byteLength).toBe(0);
      expect(deleted?.encoded?.content.byteLength).toBe(0);
      expect(deleted?.fallback).toBeUndefined();
      expect(deleted?.deliveryCursor).toBeDefined();
      expect(typeof history.cursor).toBe("string");
    } finally {
      await client.end();
      await peer?.end();
    }
  },
);
