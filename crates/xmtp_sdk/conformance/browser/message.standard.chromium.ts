import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import {
  Message,
  registerClient,
} from "../../../../target/sdk-generated/typescript-wasm/host-message.gen";
import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/public-api.gen";
import {
  currentProjection,
  liftEncodedContent,
} from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";
import { create, options, signer } from "./suite-support";

function expect(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

function contentType(typeId: string, versionMajor: number): B.ContentTypeId {
  return B.ContentTypeId.create({
    authorityId: "xmtp.org",
    typeId,
    versionMajor,
    versionMinor: 0,
  });
}

function text(content: unknown): string | undefined {
  if (content === null || typeof content !== "object") return undefined;
  if (Reflect.get(content, "tag") !== B.MessageContent_Tags.Text)
    return undefined;
  const inner: unknown = Reflect.get(content, "inner");
  return Array.isArray(inner) && typeof inner[0] === "string"
    ? inner[0]
    : undefined;
}

// The worker sends each message with the content that Rust decoded. The host
// must use that content. The encoded bytes can hold a form that the pure codec
// does not decode, such as a legacy reaction.
export function checkStandardMessageLift(): void {
  const session = {} as MainSession;
  const clientKey = 43n;
  registerClient(session, { clientKey: () => clientKey } as Client, []);
  const reference: B.MessageId = "00".repeat(32);
  const undecodable = (type: B.ContentTypeId): B.EncodedContent =>
    B.EncodedContent.create({
      type,
      content: new Uint8Array([0x1f, 0x8b, 0x08, 0xff]).buffer,
    });
  const lift = (data: Partial<B.MessageData>): Message =>
    new Message({ clientKey, ...data } as B.MessageData, session);

  const textMessage = lift({
    encoded: undecodable(contentType("text", 1)),
    content: B.MessageContent.Text.new("worker text"),
    inReplyTo: {
      encoded: undecodable(contentType("text", 1)),
      content: B.MessageBody.Text.new("worker parent"),
    } as B.ReplyParent,
  });
  expect(
    text(textMessage.content) === "worker text",
    "host decoded text again",
  );
  expect(
    textMessage.inReplyToContent?.tag === B.MessageBody_Tags.Text &&
      textMessage.inReplyToContent.inner[0] === "worker parent",
    "host decoded the reply parent again",
  );

  const reply = lift({
    encoded: undecodable(contentType("reply", 1)),
    content: B.MessageContent.Reply.new({
      referenceId: reference,
      body: B.MessageBody.Text.new("worker reply"),
    }),
  });
  expect(
    reply.replyContent?.tag === B.MessageBody_Tags.Text &&
      reply.replyContent.inner[0] === "worker reply",
    "host decoded the reply body again",
  );

  const legacy = lift({
    encoded: undecodable(contentType("reaction", 1)),
    content: B.MessageContent.Reaction.new({
      reference,
      referenceInboxId: undefined,
      reaction: {
        content: "👍",
        action: B.ReactionAction.Added,
        schema: B.ReactionSchema.Unicode,
      },
    }),
  });
  expect(
    legacy.content.tag === B.MessageContent_Tags.Reaction &&
      legacy.content.inner.reaction.content === "👍",
    "host dropped the legacy reaction",
  );
}

// verifies: CTYPE-024
// A compressed standard message, a compressed reply, and a legacy reaction
// read through the public layer with the content that Rust decoded.
export async function checkStandardMessages(backendURL: string): Promise<void> {
  await Pure.initPureWasm();
  const worker = new Worker(
    new URL("./message.deleted.worker.ts", import.meta.url),
    {
      type: "module",
    },
  );
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  let client: sdk.Client | undefined;
  let step = "create client";
  try {
    await session.ready();
    const path = `standard-${crypto.randomUUID()}.db`;
    client = (await create(session, signer(session), options(path, backendURL)))
      .client;
    step = "create group";
    const group = await client.conversations.createGroup([]);
    step = "send gzip text";
    const textId = await group.sendText("gzip text", {
      optimistic: false,
      compression: "gzip",
    });
    step = "send deflate reply";
    const replyId = await client.conversations.replyToMessage(
      textId,
      liftEncodedContent(Pure.encodeText("deflate reply"), currentProjection()),
      { optimistic: false, compression: "deflate" },
    );
    step = "send legacy reaction";
    const reactionId = await group.send({
      type: {
        authorityId: "xmtp.org",
        typeId: "reaction",
        versionMajor: 1,
        versionMinor: 0,
      },
      parameters: new Map(),
      content: new TextEncoder().encode(
        JSON.stringify({
          action: "added",
          reference: textId,
          schema: "unicode",
          content: "👍",
        }),
      ),
    });

    step = "list messages";
    const messages = await group.messages();
    const find = (id: sdk.MessageId): sdk.Message | undefined =>
      messages.find((message) => message.id === id);
    const plain = find(textId)?.content;
    expect(
      plain?.kind === "text" && plain.value === "gzip text",
      "gzip text was not decoded",
    );
    const reply = find(replyId);
    expect(
      reply?.replyContent?.kind === "text" &&
        reply.replyContent.value === "deflate reply",
      "deflate reply body was not decoded",
    );
    expect(
      reply.inReplyToContent?.kind === "text" &&
        reply.inReplyToContent.value === "gzip text",
      "gzip reply parent was not decoded",
    );

    step = "read legacy reaction";
    const reaction = await client.conversations.getMessageById(reactionId);
    expect(
      reaction?.content.kind === "reaction" &&
        reaction.content.reaction.content === "👍",
      "legacy reaction was not decoded",
    );
  } catch (error) {
    const detail =
      error instanceof sdk.XmtpError ? error.details.message : undefined;
    throw new Error(`${step}: ${String(error)}: ${String(detail)}`);
  } finally {
    await client?.end();
    worker.terminate();
  }
}
