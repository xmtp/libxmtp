import {
  Message,
  registerClient,
  unregisterClient,
} from "../../../../target/sdk-generated/typescript-wasm/host-message.gen";
import type { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import type { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function expect(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

export function checkCustomMessageLift(): void {
  const session = {} as MainSession;
  const clientKey = 41n;
  const type = B.ContentTypeId.create({
    authorityId: "example.org",
    typeId: "custom-lift",
    versionMajor: 1,
    versionMinor: 0,
  });
  const encoded = B.EncodedContent.create({
    type,
    content: new Uint8Array([1, 2, 3]).buffer,
  });
  const rawBytes = new Uint8Array([10, 3, 1, 2, 3]).buffer;
  const data = {
    clientKey,
    rawBytes,
    encoded,
    content: B.MessageContent.Custom.new({ encoded, rawBytes }),
    inReplyTo: {
      encoded,
      content: B.MessageBody.Custom.new({ encoded, rawBytes }),
    },
  } as B.MessageData;
  const noCodec = { clientKey: () => clientKey } as Client;
  registerClient(session, noCodec, []);

  const unknown = new Message(data, session);
  expect(
    unknown.content.tag === B.MessageContent_Tags.Unknown,
    "missing codec did not yield Unknown",
  );
  expect(
    unknown.replyContent === undefined,
    "synthetic message unexpectedly has reply content",
  );
  expect(
    unknown.inReplyToContent?.tag === B.MessageBody_Tags.Unknown,
    "missing reply codec did not yield Unknown",
  );
  expect(
    new Uint8Array(unknown.content.inner.rawBytes).toString() ===
      new Uint8Array(rawBytes).toString(),
    "unknown raw bytes changed",
  );

  const withCodec = { clientKey: () => clientKey } as Client;
  registerClient(session, withCodec, [
    {
      type,
      encode: () => encoded,
      decode: () => "decoded value",
    },
  ]);
  const custom = new Message(data, session);
  expect(
    custom.content.tag === B.MessageContent_Tags.Custom,
    "registered codec changed tag",
  );
  expect(
    custom.content.inner.value === "decoded value",
    "registered codec did not decode",
  );
  expect(
    new Uint8Array(custom.content.inner.rawBytes).toString() ===
      new Uint8Array(rawBytes).toString(),
    "custom raw bytes changed",
  );

  // A deleted message keeps its encoded bytes. The host must use Rust's
  // deleted marker, also when a registered codec could decode those bytes.
  const deleted = new Message(
    {
      ...data,
      content: B.MessageContent.DeletedMessage.new({
        deletedBy: B.DeletedBy.Sender.new(),
      }),
    } as B.MessageData,
    session,
  );
  expect(
    deleted.content.tag === B.MessageContent_Tags.DeletedMessage,
    "the host lifted deleted custom bytes as content",
  );

  unregisterClient(session, clientKey);
  const closed = new Message(data, session);
  expect(
    closed.content.tag === B.MessageContent_Tags.Custom,
    "closed client changed tag",
  );
  expect(
    closed.content.inner.error?.code === "ClientClosed",
    "closed client error was lost",
  );
  expect(
    new Uint8Array(closed.content.inner.rawBytes).toString() ===
      new Uint8Array(rawBytes).toString(),
    "closed client raw bytes changed",
  );
}
