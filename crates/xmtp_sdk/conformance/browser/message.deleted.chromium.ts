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
  Client,
  Message,
} from "../../../../target/sdk-generated/typescript-wasm/index";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function deleted(message: Message | undefined, label: string): void {
  if (!message) throw new Error(`${label} was not found`);
  if (message.content.tag !== B.MessageContent_Tags.DeletedMessage)
    throw new Error(
      `${label} exposed ${message.content.tag} instead of DeletedMessage`,
    );
}

// A deleted message keeps its original encoded bytes. The host content must
// come from Rust's deleted marker, including when the bytes use a custom type.
export async function checkDeletedMessages(backendURL: string): Promise<void> {
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
    const path = `deleted-${crypto.randomUUID()}.db`;
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
    step = "send text";
    const textId = await group.sendText("secret text", undefined);
    step = "send reply";
    const replyId = await client
      .conversations()
      .replyToMessage(textId, Pure.encodeText("reply"), undefined);
    step = "delete text";
    await client.conversations().deleteMessage(textId);
    step = "sync deleted text";
    await group.sync();

    step = "read deleted text";
    const deletedText = await client.conversations().getMessageById(textId);
    deleted(deletedText, "text lookup");
    deleted(
      (await group.messages(undefined)).find(
        (message) => message.id.toString() === textId.toString(),
      ),
      "text list",
    );
    const reply = await client.conversations().getMessageById(replyId);
    if (reply?.inReplyToContent?.tag !== B.MessageBody_Tags.DeletedMessage)
      throw new Error("reply parent exposed the deleted text");

    const customType = B.ContentTypeId.create({
      authorityId: "example.org",
      typeId: "deleted-browser-content",
      versionMajor: 1,
      versionMinor: 0,
    });
    step = "send custom content";
    const customId = await group.send(
      B.EncodedContent.create({
        type: customType,
        content: new TextEncoder().encode("secret custom").buffer,
      }),
      undefined,
    );
    step = "decode deleted custom bytes";
    const custom = await client.conversations().getMessageById(customId);
    if (!custom || !deletedText)
      throw new Error("message fixture was not found");
    // Core does not allow deleting an unknown custom type. Lift its real encoded
    // bytes with the deleted marker from the text message to test the host path.
    deleted(
      new Message(
        B.MessageData.create({ ...custom.data, content: deletedText.content }),
        session,
      ),
      "deleted custom bytes",
    );
  } catch (error) {
    const inner =
      error !== null && typeof error === "object"
        ? Reflect.get(error, "inner")
        : undefined;
    const detail =
      Array.isArray(inner) && inner[0] !== null && typeof inner[0] === "object"
        ? Reflect.get(inner[0], "message")
        : undefined;
    throw new Error(`${step}: ${String(error)}: ${String(detail)}`);
  } finally {
    await client?.end();
    worker.terminate();
  }
}
