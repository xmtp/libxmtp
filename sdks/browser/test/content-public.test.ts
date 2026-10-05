import type { EncodedContent, MessageContent } from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

import { create, signer } from "./helpers";

beforeAll(() => initPureWasm());

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
