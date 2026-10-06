import {
  createRegisteredClient,
  createSigner,
  DecodeFailureCodec,
  TestCodec,
} from "@test/helpers";
import * as sdk from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import {
  standardCodecs,
  standardSamples,
  variantSamples,
} from "./standardSamples";

const valueOf = (content: sdk.StandardContent) =>
  content.kind === "readReceipt"
    ? undefined
    : content.kind === "reaction" ||
        content.kind === "reply" ||
        content.kind === "deleteMessage"
      ? content
      : content.value;
describe("Content types", () => {
  it.each([...standardSamples, ...variantSamples])(
    "preserves the Rust envelope and hooks for $kind",
    (content) => {
      const encoded = sdk.encodeStandard(content);
      const codec = standardCodecs.find(
        (c) => c.type.typeId === encoded.type.typeId,
      )!;
      const value = valueOf(content);
      expect(codec.encode(value as never)).toEqual(encoded);
      expect(codec.fallback?.(value as never)).toBe(encoded.fallback);
      expect(codec.shouldPush?.(value as never)).toBe(
        sdk.catalogueContentTypeShouldPush(codec.type),
      );
      expect(codec.encode(codec.decode(encoded) as never)).toEqual(encoded);
      expect(
        sdk.decodeEncodedContent(sdk.encodeEncodedContent(encoded)),
      ).toEqual(encoded);
    },
  );

  it("lifts every sendable standard kind and a deletion into the read message", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    try {
      const group = await client.conversations.createGroup([]);
      const sendable = [...standardSamples, ...variantSamples].filter(
        (c) =>
          ![
            "groupUpdated",
            "deleteMessage",
            "leaveRequest",
            "reaction",
            "reply",
            "readReceipt",
          ].includes(c.kind),
      );
      for (const content of sendable) {
        const id = await group.send(sdk.encodeStandard(content));
        const message = await client.conversations.getMessageById(id);
        expect(message?.contentType).toEqual(
          sdk.standardContentType(content.kind),
        );
        expect(message?.content).toEqual({
          kind: content.kind,
          value: valueOf(sdk.decodeStandard(sdk.encodeStandard(content))),
        });
      }

      // A delete is not a sendable kind: the hand-written Message.delete()
      // wrapper sends it, and the read lifts the deletedMessage variant.
      const deletedId = await group.sendText("delete this message");
      const deleted = await client.conversations.getMessageById(deletedId);
      expect(deleted!.rawBytes).toBeInstanceOf(Uint8Array);
      expect(deleted!.rawBytes.byteLength).toBeGreaterThan(0);
      const events = await client.events({
        kinds: ["message.deleted"],
        references_own_messages: false,
      });
      try {
        expect(typeof (await deleted!.delete())).toBe("string");
        // The generated event lift gives the IDs as bytes.
        const next = await events.next();
        if (next.done || next.value.kind !== "message.deleted")
          throw new Error("expected a deletion event");
        expect(next.value.message_deleted.message_id).toBeInstanceOf(
          Uint8Array,
        );
        expect(Buffer.from(next.value.message_deleted.message_id)).toEqual(
          Buffer.from(deletedId, "hex"),
        );
        expect(Buffer.from(next.value.message_deleted.group_id)).toEqual(
          Buffer.from(group.id, "hex"),
        );
      } finally {
        await events.return();
      }
      const read = await client.conversations.getMessageById(deletedId);
      expect(read?.content).toEqual({
        kind: "deletedMessage",
        value: { deletedBy: { kind: "sender" } },
      });
      expect(read?.rawBytes.byteLength).toBe(0);
      expect(read?.fallback).toBeUndefined();
    } finally {
      await client.end();
    }
  });

  it.each(["description", "transactionType"] as const)(
    "rejects wallet metadata with missing %s before publishing",
    async (missing) => {
      const client = await createRegisteredClient(createSigner().signer);
      try {
        const group = await client.conversations.createGroup([]);
        const before = await group.countMessages(undefined);
        const metadata: Partial<sdk.WalletCallMetadata> = {
          description: "Transfer",
          transactionType: "transfer",
          extra: new Map(),
        };
        delete metadata[missing];
        await expect(
          group.sendWalletSendCalls({
            version: "1.0",
            chainId: "1",
            from: "0x1234567890",
            calls: [
              {
                to: "0x1234567890",
                data: "0x1234567890",
                value: "0x1234567890",
                metadata: metadata as sdk.WalletCallMetadata,
              },
            ],
          }),
        ).rejects.toThrow();
        expect(await group.countMessages(undefined)).toBe(before);
      } finally {
        await client.end();
      }
    },
  );

  it("lifts reactions into the read message and onto the parent", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    try {
      const group = await client.conversations.createGroup([]);
      const parent = await group.sendText("parent");
      const reactions = [
        { action: "added", schema: "unicode", content: "👍" },
        { action: "added", schema: "shortcode", content: ":thumbsup:" },
        { action: "removed", schema: "custom", content: "thumbsup" },
      ] as const;
      const ids: string[] = [];
      for (const reaction of reactions) {
        const id = await group.sendReaction(parent, client.inboxId, reaction);
        ids.push(id);
        expect(
          (await client.conversations.getMessageById(id))?.content,
        ).toEqual({
          kind: "reaction",
          reference: parent,
          referenceInboxId: client.inboxId,
          reaction,
        });
      }
      const original = await client.conversations.getMessageById(parent);
      expect(original?.reactions.map((reaction) => reaction.id)).toEqual(ids);
    } finally {
      await client.end();
    }
  });

  it("retains replies with text, attachment, and custom bodies", async () => {
    const codec = new TestCodec();
    const client = await createRegisteredClient(createSigner().signer, {
      codecs: [codec],
    });
    const group = await client.conversations.createGroup([]);
    const parent = await group.sendText("parent");
    const bodies = [
      new sdk.TextCodec().encode("reply"),
      new sdk.AttachmentCodec().encode({
        mimeType: "text/plain",
        content: new Uint8Array([1, 2]),
      }),
      codec.encode({ test: "value" }),
    ];
    for (const body of bodies) {
      const id = await group.sendReply(parent, client.inboxId, body);
      const reply = await client.conversations.getMessageById(id);
      expect(reply?.content.kind).toBe("reply");
      if (reply?.content.kind !== "reply") throw new Error("Expected reply");
      expect(reply.content.referenceId).toBe(parent);
      if (body.type.typeId === codec.type.typeId)
        expect(reply.content.body).toMatchObject({
          kind: "custom",
          encoded: body,
          value: codec.decode(body),
        });
      else expect(reply.content.body).toEqual(await client.decodeContent(body));
      expect(reply.inReplyTo?.id).toBe(parent);
    }
    expect(
      (await client.conversations.getMessageById(parent))?.replyCount,
    ).toBe(3n);
  });

  it.each([true, false])(
    "retains attachment optional filename %s through encryption",
    async (named) => {
      const value = {
        mimeType: "text/plain",
        content: new Uint8Array([1, 2, 3]),
        ...(named ? { filename: "test.txt" } : {}),
      };
      const encoded = new sdk.AttachmentCodec().encode(value);
      const encrypted = await sdk.encryptEncodedContent(
        sdk.encodeEncodedContent(encoded),
      );
      const decrypted = await sdk.decryptEncodedContent(encrypted);
      expect(
        new sdk.AttachmentCodec().decode(sdk.decodeEncodedContent(decrypted)),
      ).toEqual(value);
    },
  );

  // verifies: CTYPE-017
  it("reports missing and failed custom codecs without losing the raw envelope", async () => {
    const codec = new TestCodec();
    const failing = new DecodeFailureCodec();
    const sender = await createRegisteredClient(createSigner().signer, {
      codecs: [codec, failing],
    });
    const receiver = await createRegisteredClient(createSigner().signer, {
      codecs: [failing],
    });
    const group = await sender.conversations.createGroup([receiver.inboxId]);
    const unknown = codec.encode({ test: "unknown" });
    const bad = failing.encode("failure");
    const ids = [
      await group.send(unknown),
      await group.send(bad),
      await group.sendText("after failure"),
    ];
    await receiver.conversations.sync();
    const peer = await receiver.conversations.getById(group.id);
    await peer!.sync();
    const messages = await Promise.all(
      ids.map((id) => receiver.conversations.getMessageById(id)),
    );
    expect(messages[0]?.content).toMatchObject({
      kind: "unknown",
      encoded: unknown,
      error: { code: "CodecNotFound" },
    });
    expect(messages[1]?.content).toMatchObject({
      kind: "custom",
      encoded: bad,
      error: { code: "CodecDecodeFailed" },
    });
    expect(messages[2]?.content).toEqual({
      kind: "text",
      value: "after failure",
    });
    expect(
      (await sender.conversations.getMessageById(ids[0]))?.content,
    ).toMatchObject({ kind: "custom", value: { test: "unknown" } });
  });

  // The Node wrapper decodes custom reply bodies and parents with the
  // client's codecs (runtime/ts/message.ts); Rust decodes only standard ones.
  // verifies: CTYPE-029
  it("keeps the outer reply when a custom reply body or parent fails to decode", async () => {
    const failing = new DecodeFailureCodec();
    const sender = await createRegisteredClient(createSigner().signer);
    const receiver = await createRegisteredClient(createSigner().signer, {
      codecs: [failing],
    });
    try {
      const group = await sender.conversations.createGroup([receiver.inboxId]);
      const text = await group.sendText("text parent");
      const custom = await group.send(failing.encode("custom parent"));
      const badReply = await group.sendReply(
        text,
        sender.inboxId,
        failing.encode("custom reply"),
      );
      const textReply = await group.sendReply(
        custom,
        sender.inboxId,
        new sdk.TextCodec().encode("text reply"),
      );
      await receiver.conversations.sync();
      await (await receiver.conversations.getById(group.id))!.sync();
      const read = (id: string) => receiver.conversations.getMessageById(id);
      const sent = (await sender.conversations.getMessageById(badReply))!;

      // A failed custom reply body reads as unknown with the outer envelope.
      const failed = await read(badReply);
      expect(failed?.content).toMatchObject({
        kind: "unknown",
        error: { code: "CodecDecodeFailed", category: "callback" },
      });
      if (failed?.content.kind !== "unknown") throw new Error("not unknown");
      expect(failed.content.rawBytes).toEqual(sent.rawBytes);
      expect(failed.content.encoded?.fallback).toBe(sent.fallback);
      expect(sent.fallback).toBeTruthy();

      // A text reply keeps its body; its failed custom parent is isolated.
      const parent = await read(custom);
      if (parent?.content.kind !== "custom") throw new Error("not custom");
      const reply = await read(textReply);
      expect(reply?.content).toMatchObject({
        kind: "reply",
        body: { kind: "text", value: "text reply" },
      });
      expect(reply?.replyContent).toEqual({
        kind: "text",
        value: "text reply",
      });
      expect(reply?.inReplyToContent).toMatchObject({
        kind: "custom",
        error: { code: "CodecDecodeFailed", category: "callback" },
      });
      if (reply?.inReplyToContent?.kind !== "custom")
        throw new Error("parent is not custom");
      expect(reply.inReplyToContent.rawBytes).toEqual(parent.content.rawBytes);
    } finally {
      await Promise.all([sender.end(), receiver.end()]);
    }
  });
});
