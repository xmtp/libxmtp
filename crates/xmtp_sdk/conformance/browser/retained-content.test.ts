import { beforeAll, describe, expect, it } from "vitest";

import {
  Message as HostMessage,
  registerClient,
  unregisterClient,
} from "../../../../target/sdk-generated/typescript-wasm/host-message.gen";
import type { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import type { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import { liftBoundMessage } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/message";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

class TestProjection extends P.ObjectProjection {
  liftMessage(): never {
    throw new Error("unexpected object lift");
  }
  lowerMessage(): never {
    throw new Error("unexpected object lower");
  }
}
beforeAll(() => P.installProjection(new TestProjection()));

const type = B.ContentTypeId.create({
  authorityId: "custom.example",
  typeId: "note",
  versionMajor: 1,
  versionMinor: 0,
});
const encoded = B.EncodedContent.create({
  type,
  content: new Uint8Array([1, 2]).buffer,
  fallback: "nested fallback",
});
const nestedRaw = new Uint8Array([10, 1, 2, 99]).buffer;
const outerRaw = new Uint8Array([10, 9, 8, 7, 99]).buffer;
const error = B.ErrorDetails.create({
  code: "MalformedEnvelope",
  category: B.ErrorCategory.Input,
  retryable: false,
  message: "invalid protobuf",
});
function data(
  content: B.MessageContent,
  overrides: Partial<B.MessageData> = {},
): B.MessageData {
  return B.MessageData.create({
    id: "01",
    clientKey: 31n,
    conversationId: "group",
    topic: "topic",
    senderInboxId: "inbox",
    sentAt: new P.Timestamp(1n),
    insertedAt: new P.Timestamp(2n),
    kind: B.MessageKind.Application,
    deliveryStatus: B.DeliveryStatus.Published,
    rawBytes: outerRaw,
    contentType: type,
    encoded,
    content,
    replyCount: 0n,
    reactions: [],
    ...overrides,
  });
}
function custom(): B.MessageContent {
  return B.MessageContent.Custom.new({ encoded, rawBytes: nestedRaw });
}
function reply(): B.MessageContent {
  return B.MessageContent.Reply.new({
    referenceId: "02",
    body: B.MessageBody.Custom.new({ encoded, rawBytes: nestedRaw }),
  });
}
function host(
  session: MainSession,
  content: B.MessageContent,
  overrides: Partial<B.MessageData> = {},
) {
  return liftBoundMessage(new HostMessage(data(content, overrides), session));
}
function owner(session: MainSession, decode?: () => unknown): Client {
  const client = { clientKey: () => 31n } as Client;
  registerClient(
    session,
    client,
    decode === undefined ? [] : [{ type, encode: () => encoded, decode }],
  );
  return client;
}

// verifies: CTYPE-008, CTYPE-009, CTYPE-027, CTYPE-029
// These calls use the public Message lift after the browser host decodes.
describe("retained received content", () => {
  it("exposes exact malformed bytes with no fabricated envelope or type", () => {
    const session = {} as MainSession;
    const message = host(
      session,
      B.MessageContent.Unknown.new({ rawBytes: outerRaw, error }),
      { contentType: undefined, encoded: undefined },
    );
    expect(message.contentType).toBeUndefined();
    expect(message.encoded).toBeUndefined();
    expect(message.rawBytes).toEqual(new Uint8Array(outerRaw));
    expect(message.content).toEqual({
      kind: "unknown",
      encoded: undefined,
      rawBytes: new Uint8Array(outerRaw),
      error: {
        code: "MalformedEnvelope",
        category: "input",
        retryable: false,
        message: "invalid protobuf",
      },
    });
  });

  it("keeps a missing nested codec as Reply with an Unknown body", () => {
    const session = {} as MainSession;
    const client = owner(session);
    const message = host(session, reply());
    expect(message.content.kind).toBe("reply");
    if (message.content.kind !== "reply") throw new Error("reply missing");
    expect(message.content.body.kind).toBe("unknown");
    if (message.content.body.kind !== "unknown")
      throw new Error("unknown body missing");
    expect(message.content.body.rawBytes).toEqual(new Uint8Array(nestedRaw));
    expect(message.content.body.error).toMatchObject({
      code: "CodecNotFound",
      category: "input",
      retryable: false,
    });
    expect(message.content.body.encoded?.fallback).toBe("nested fallback");
    unregisterClient(session, client.clientKey());
  });

  it("keeps top-level host failure details and calls the decoder once", () => {
    const session = {} as MainSession;
    let calls = 0;
    const client = owner(session, () => {
      calls++;
      throw new Error("custom payload failed");
    });
    const message = host(session, custom());
    expect(calls).toBe(1);
    expect(message.content.kind).toBe("custom");
    if (message.content.kind !== "custom") throw new Error("custom missing");
    expect(message.content.value).toBeUndefined();
    expect(message.content.rawBytes).toEqual(new Uint8Array(nestedRaw));
    expect(message.content.error).toMatchObject({
      code: "CodecDecodeFailed",
      category: "callback",
      retryable: false,
      message: "Error: custom payload failed",
    });
    unregisterClient(session, client.clientKey());
  });

  it("promotes a throwing nested host codec to outer Unknown with outer bytes", () => {
    const session = {} as MainSession;
    let calls = 0;
    const client = owner(session, () => {
      calls++;
      throw new Error("nested codec failed");
    });
    const outer = B.EncodedContent.create({
      ...encoded,
      fallback: "outer fallback",
    });
    const message = host(session, reply(), {
      encoded: outer,
      fallback: "outer fallback",
    });
    expect(calls).toBe(1);
    expect(message.content.kind).toBe("unknown");
    if (message.content.kind !== "unknown")
      throw new Error("outer Unknown missing");
    expect(message.content.rawBytes).toEqual(new Uint8Array(outerRaw));
    expect(message.content.encoded?.fallback).toBe("outer fallback");
    expect(message.fallback).toBe("outer fallback");
    expect(message.content.error).toMatchObject({
      code: "CodecDecodeFailed",
      category: "callback",
      retryable: false,
    });
    unregisterClient(session, client.clientKey());
  });

  it("limits a parent codec failure to its parent", () => {
    const session = {} as MainSession;
    const client = owner(session, () => {
      throw new Error("parent codec failed");
    });
    const parent = B.ReplyParent.create({
      id: "02",
      senderInboxId: "inbox",
      sentAt: new P.Timestamp(1n),
      kind: B.MessageKind.Application,
      deliveryStatus: B.DeliveryStatus.Published,
      rawBytes: nestedRaw,
      contentType: type,
      encoded,
      content: B.MessageBody.Custom.new({ encoded, rawBytes: nestedRaw }),
    });
    const message = host(session, B.MessageContent.Text.new("valid reply"), {
      inReplyTo: parent,
    });
    expect(message.content).toEqual({ kind: "text", value: "valid reply" });
    expect(message.inReplyTo?.rawBytes).toEqual(new Uint8Array(nestedRaw));
    expect(message.inReplyToContent?.kind).toBe("custom");
    if (message.inReplyToContent?.kind !== "custom")
      throw new Error("parent custom missing");
    expect(message.inReplyToContent.error).toMatchObject({
      code: "CodecDecodeFailed",
      category: "callback",
    });
    expect(message.inReplyToContent.rawBytes).toEqual(
      new Uint8Array(nestedRaw),
    );
    unregisterClient(session, client.clientKey());
  });

  it("records a closed owner with typed details and retains bytes", () => {
    const message = host({} as MainSession, custom());
    expect(message.content.kind).toBe("custom");
    if (message.content.kind !== "custom") throw new Error("custom missing");
    expect(message.content.rawBytes).toEqual(new Uint8Array(nestedRaw));
    expect(message.content.error).toMatchObject({
      code: "ClientClosed",
      category: "lifecycle",
      retryable: false,
    });
  });
});
