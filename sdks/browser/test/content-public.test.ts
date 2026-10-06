import type {
  ContentCodec,
  EncodedContent,
  MessageContent,
} from "@xmtp/browser-sdk";
import {
  initPureWasm,
  ReactionV2Codec,
  RemoteAttachmentCodec,
  TextCodec,
  TransactionReferenceCodec,
  WalletSendCallsCodec,
} from "@xmtp/browser-sdk/pure";
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

test("a peer worker projects standard content, content type, and fallback", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  const parent = await group.sendText("parent", { shouldPush: false });
  const sample = <T>(
    codec: ContentCodec<T>,
    value: T,
    content: MessageContent,
  ) => ({
    content,
    type: codec.type,
    fallback: codec.encode(value).fallback,
    send: () => group.send(codec, value, { shouldPush: false }),
  });
  const remote = {
    url: "https://example.com/file",
    scheme: "https",
    contentDigest: "digest",
    secret: new Uint8Array(32).fill(1),
    salt: new Uint8Array(32).fill(2),
    nonce: new Uint8Array(12).fill(3),
    contentLength: 3,
    filename: "image.png",
  };
  const transaction = {
    networkId: "1",
    reference: "0x123",
    namespace: "eip155",
    metadata: {
      transactionType: "transfer",
      currency: "ETH",
      amount: 1,
      decimals: 18,
      fromAddress: "0xabc",
      toAddress: "0xdef",
    },
  };
  const wallet = {
    version: "1.0",
    chainId: "0x1",
    from: "0xabc",
    calls: [
      {
        to: "0xdef",
        data: "0x01",
        value: "0x1",
        gas: "0x5208",
        metadata: {
          description: "Transfer",
          transactionType: "transfer",
          extra: new Map([["currency", "ETH"]]),
        },
      },
    ],
    capabilities: new Map([
      ["paymasterService", '{"url":"https://example.com"}'],
    ]),
  };
  const reaction = {
    kind: "reaction",
    reference: parent,
    referenceInboxId: sender.inboxId,
    reaction: { action: "added", schema: "unicode", content: "👍" },
  } as const;
  const samples = [
    sample(new TextCodec(), "text", { kind: "text", value: "text" }),
    sample(new ReactionV2Codec(), reaction, reaction),
    sample(new RemoteAttachmentCodec(), remote, {
      kind: "remoteAttachment",
      value: remote,
    }),
    sample(new TransactionReferenceCodec(), transaction, {
      kind: "transactionReference",
      value: transaction,
    }),
    sample(new WalletSendCallsCodec(), wallet, {
      kind: "walletSendCalls",
      value: wallet,
    }),
  ];
  const sent = [];
  for (const item of samples) sent.push({ ...item, id: await item.send() });
  await peer.conversations.syncAll(undefined);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Peer group missing");
  const listed = await received.messages();
  for (const item of sent) {
    const lookup = await peer.conversations.getMessageById(item.id);
    const row = listed.find((value) => value.id === item.id);
    // History attaches a reaction to its parent and omits the reaction row.
    const reads = item.content.kind === "reaction" ? [lookup] : [lookup, row];
    for (const message of reads) {
      expect(message?.id).toBe(item.id);
      expect(message?.senderInboxId).toBe(sender.inboxId);
      expect(message?.contentType).toEqual(item.type);
      expect(message?.content).toEqual(item.content);
      expect(message?.fallback).toBe(item.fallback);
    }
  }
});
