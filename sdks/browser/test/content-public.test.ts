import {
  Timestamp,
  type EncodedContent,
  type MessageContent,
} from "@xmtp/browser-sdk";
import {
  AttachmentCodec,
  MarkdownCodec,
  ReactionV2Codec,
  initPureWasm,
  TextCodec,
} from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

import { create, signer } from "./helpers";

beforeAll(() => initPureWasm());

test("catalogue messages preserve read times, reaction variants, markdown, and attachment replies", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  const parent = await group.send(new TextCodec(), "parent", {
    shouldPush: true,
  });
  const markdown = await group.send(new MarkdownCodec(), "**message**", {
    shouldPush: true,
  });
  expect(
    (await sender.conversations.getMessageById(markdown))?.content,
  ).toEqual({ kind: "markdown", value: "**message**" });
  for (const action of ["added", "removed"] as const) {
    for (const schema of ["unicode", "shortcode", "custom"] as const) {
      const reaction = { content: "reaction", action, schema };
      const id = await group.send(new ReactionV2Codec(), {
        kind: "reaction",
        reference: parent,
        referenceInboxId: sender.inboxId,
        reaction,
      });
      expect(
        (await sender.conversations.getMessageById(id))?.content,
      ).toMatchObject({ kind: "reaction", reference: parent, reaction });
    }
  }
  const attachment = {
    mimeType: "image/png",
    content: new Uint8Array([1, 2, 3]),
  };
  const reply = await group.sendReply(
    parent,
    sender.inboxId,
    new AttachmentCodec().encode(attachment),
  );
  expect(
    (await sender.conversations.getMessageById(reply))?.content,
  ).toMatchObject({
    kind: "reply",
    referenceId: parent,
    body: { kind: "attachment", value: attachment },
  });
  const receipt = await group.sendReadReceipt();
  const receiptMessage = await sender.conversations.getMessageById(receipt);
  expect(receiptMessage?.content).toEqual({ kind: "readReceipt" });
  expect(receiptMessage?.contentType).toEqual({
    authorityId: "xmtp.org",
    typeId: "readReceipt",
    versionMajor: 1,
    versionMinor: 0,
  });
  expect(
    (await group.messages()).some((message) => message.id === receipt),
  ).toBe(false);
  expect((await group.lastReadTimes()).get(sender.inboxId)).toBeInstanceOf(
    Timestamp,
  );
  await peer.conversations.syncAll(undefined);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Peer group missing");
  await received.sendReadReceipt();
  expect([...(await received.lastReadTimes()).keys()]).toEqual(
    expect.arrayContaining([sender.inboxId, peer.inboxId]),
  );
});

test("custom codecs keep unknown bytes, app values, and nested decode failures", async () => {
  const type = {
    authorityId: "tests.xmtp.org",
    typeId: "object-literal",
    versionMajor: 1,
    versionMinor: 0,
  };
  const codec = {
    type,
    encode(value: string): EncodedContent {
      return {
        type,
        content: new TextEncoder().encode(value),
        parameters: new Map(),
        fallback: "custom fallback",
      };
    },
    decode(encoded: EncodedContent) {
      return new TextDecoder().decode(encoded.content);
    },
  };
  const sender = await create(signer(), { codecs: [codec] });
  const peerOwner = signer();
  const missing = await create(peerOwner);
  const decoded = await create(peerOwner, { codecs: [codec] });
  const failed = await create(peerOwner, {
    codecs: [
      {
        ...codec,
        decode() {
          throw new Error("cannot decode");
        },
      },
    ],
  });
  const group = await sender.conversations.createGroup([missing.inboxId]);
  const customId = await group.send(codec, "custom value", {
    shouldPush: true,
  });
  const parentId = await group.send(new TextCodec(), "parent", {
    shouldPush: true,
  });
  const replyId = await group.sendReply(
    parentId,
    sender.inboxId,
    codec.encode("reply value"),
    { shouldPush: true },
  );
  const senderCustom = await sender.conversations.getMessageById(customId);
  const senderReply = await sender.conversations.getMessageById(replyId);
  const contents: MessageContent[] = [];
  for (const peer of [missing, decoded, failed]) {
    await peer.conversations.sync();
    const received = await peer.conversations.getById(group.id);
    if (!received) throw new Error("Peer group missing");
    await received.sync();
    const custom = await peer.conversations.getMessageById(customId);
    if (!custom) throw new Error("Custom message missing");
    expect(custom.rawBytes).toEqual(senderCustom?.rawBytes);
    expect(custom.fallback).toBe("custom fallback");
    contents.push(custom.content);
    const reply = await peer.conversations.getMessageById(replyId);
    if (peer === failed) {
      expect(reply?.content).toMatchObject({
        kind: "unknown",
        error: { code: "CodecDecodeFailed" },
      });
      expect(reply?.rawBytes).toEqual(senderReply?.rawBytes);
      expect(reply?.fallback).toBe(senderReply?.fallback);
      continue;
    }
    expect(reply?.content.kind).toBe("reply");
    if (reply?.content.kind !== "reply") throw new Error("Reply missing");
    const body = reply.content.body;
    if (peer === missing)
      expect(body).toMatchObject({
        kind: "unknown",
        error: { code: "CodecNotFound" },
      });
    if (peer === decoded)
      expect(body).toMatchObject({ kind: "custom", value: "reply value" });
  }
  expect(contents[0]).toMatchObject({
    kind: "unknown",
    encoded: { type },
    error: { code: "CodecNotFound" },
  });
  expect(contents[1]).toMatchObject({ kind: "custom", value: "custom value" });
  expect(contents[2]).toMatchObject({
    kind: "custom",
    error: { code: "CodecDecodeFailed" },
  });
});
