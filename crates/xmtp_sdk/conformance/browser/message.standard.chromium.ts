// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
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
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function expect(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

function contentType(typeID: string, versionMajor: number): B.ContentTypeID {
  return B.ContentTypeID.create({
    authorityID: "xmtp.org",
    typeID,
    versionMajor,
    versionMinor: 0,
  });
}

function text(content: unknown): string | undefined {
  if (content === null || typeof content !== "object") return undefined;
  if (Reflect.get(content, "tag") !== B.MessageContent_Tags.Text) return undefined;
  const inner: unknown = Reflect.get(content, "inner");
  return Array.isArray(inner) && typeof inner[0] === "string" ? inner[0] : undefined;
}

// The worker sends each message with the content that Rust decoded. The host
// must use that content. The encoded bytes can hold a form that the pure codec
// does not decode, such as a legacy reaction.
export function checkStandardMessageLift(): void {
  const session = {} as MainSession;
  const clientKey = 43n;
  registerClient(session, { clientKey: () => clientKey } as Client, []);
  const reference = B.MessageID.fromRust("00".repeat(32));
  const undecodable = (type: B.ContentTypeID): B.EncodedContent =>
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
  expect(text(textMessage.content) === "worker text", "host decoded text again");
  expect(
    textMessage.inReplyToContent?.tag === B.MessageBody_Tags.Text &&
      textMessage.inReplyToContent.inner[0] === "worker parent",
    "host decoded the reply parent again",
  );

  const reply = lift({
    encoded: undecodable(contentType("reply", 1)),
    content: B.MessageContent.Reply.new({
      referenceID: reference,
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
      referenceInboxID: undefined,
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
// read through the bridge with the content that Rust decoded.
export async function checkStandardMessages(backendURL: string): Promise<void> {
  await Pure.initPureWasm();
  const worker = new Worker(new URL("./message.deleted.worker.ts", import.meta.url), {
    type: "module",
  });
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
  let client: Client | undefined;
  let step = "create client";
  try {
    await session.ready();
    const account = privateKeyToAccount(generatePrivateKey());
    const signer = {
      async identity() {
        return {
          identifier: account.address.toLowerCase(),
          kind: B.PublicIdentityKind.Ethereum,
        };
      },
      async kind() {
        return B.SignerKind.Eoa.new();
      },
      async sign(request: { text: string }) {
        const signed = await account.signMessage({ message: request.text });
        return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
      },
    };
    const path = `standard-${crypto.randomUUID()}.db`;
    client = await Client.create(session, signer, {
      backend: B.BackendSource.Options.new({
        options: {
          url: backendURL,
          appVersion: undefined,
          credential: undefined,
          credentials: undefined,
        },
      }),
      storage: {
        location: B.StorageLocation.Path.new(path),
        label: path,
        pool: undefined,
        singleConnection: false,
      },
      deviceSync: false,
      registration: { auto: true, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
    });
    step = "create group";
    const group = await client.conversations().createGroup([], undefined);
    step = "send gzip text";
    const textID = await group.sendText("gzip text", {
      optimistic: false,
      compression: B.Compression.Gzip,
    });
    step = "send deflate reply";
    const replyID = await client.conversations().replyToMessage(
      textID,
      Pure.encodeText("deflate reply"),
      { optimistic: false, compression: B.Compression.Deflate },
    );
    step = "send legacy reaction";
    const reactionID = await group.send(
      B.EncodedContent.create({
        type: contentType("reaction", 1),
        content: new TextEncoder().encode(
          JSON.stringify({
            action: "added",
            reference: textID.toString(),
            schema: "unicode",
            content: "👍",
          }),
        ).buffer,
      }),
      undefined,
    );

    step = "list messages";
    const messages = await group.messages(undefined);
    const find = (id: B.MessageID): Message | undefined =>
      messages.find((message) => message.id.toString() === id.toString());
    expect(text(find(textID)?.content) === "gzip text", "gzip text was not decoded");
    const reply = find(replyID);
    expect(
      reply?.replyContent?.tag === B.MessageBody_Tags.Text &&
        reply.replyContent.inner[0] === "deflate reply",
      "deflate reply body was not decoded",
    );
    expect(
      reply.inReplyToContent?.tag === B.MessageBody_Tags.Text &&
        reply.inReplyToContent.inner[0] === "gzip text",
      "gzip reply parent was not decoded",
    );

    step = "read legacy reaction";
    const reaction = await client.conversations().getMessageByID(reactionID);
    expect(
      reaction?.content.tag === B.MessageContent_Tags.Reaction &&
        reaction.content.inner.reaction.content === "👍",
      "legacy reaction was not decoded",
    );
  } catch (error) {
    const inner = error !== null && typeof error === "object"
      ? Reflect.get(error, "inner")
      : undefined;
    const detail = Array.isArray(inner) && inner[0] !== null && typeof inner[0] === "object"
      ? Reflect.get(inner[0], "message")
      : undefined;
    throw new Error(`${step}: ${String(error)}: ${String(detail)}`);
  } finally {
    await client?.end();
    worker.terminate();
  }
}
