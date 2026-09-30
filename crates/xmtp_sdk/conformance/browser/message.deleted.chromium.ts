import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { Message as HostMessage } from "../../../../target/sdk-generated/typescript-wasm/host-message.gen";
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
import { boundMessage } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/message";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";
import { create, options, signer } from "./suite-support";

function deleted(message: sdk.Message | undefined, label: string): void {
  if (!message) throw new Error(`${label} was not found`);
  if (message.content.kind !== "deletedMessage")
    throw new Error(
      `${label} exposed ${message.content.kind} instead of deletedMessage`,
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
  let client: sdk.Client | undefined;
  let step = "create client";
  try {
    await session.ready();
    const path = `deleted-${crypto.randomUUID()}.db`;
    client = (await create(session, signer(session), options(path, backendURL)))
      .client;
    step = "create group";
    const group = await client.conversations.createGroup([]);
    step = "send text";
    const textId = await group.sendText("secret text");
    step = "send reply";
    const replyId = await client.conversations.replyToMessage(
      textId,
      liftEncodedContent(Pure.encodeText("reply"), currentProjection()),
    );
    step = "delete text";
    await client.conversations.deleteMessage(textId);
    step = "sync deleted text";
    await group.sync();

    step = "read deleted text";
    const deletedText = await client.conversations.getMessageById(textId);
    deleted(deletedText, "text lookup");
    deleted(
      (await group.messages()).find((message) => message.id === textId),
      "text list",
    );
    const reply = await client.conversations.getMessageById(replyId);
    if (reply?.inReplyToContent?.kind !== "deletedMessage")
      throw new Error("reply parent exposed the deleted text");

    step = "send custom content";
    const customId = await group.send({
      type: {
        authorityId: "example.org",
        typeId: "deleted-browser-content",
        versionMajor: 1,
        versionMinor: 0,
      },
      parameters: new Map(),
      content: new TextEncoder().encode("secret custom"),
    });
    step = "decode deleted custom bytes";
    const custom = await client.conversations.getMessageById(customId);
    if (!custom || !deletedText)
      throw new Error("message fixture was not found");
    // Host level: core does not allow deleting an unknown custom type. Lift its
    // real encoded bytes with the deleted marker from the text message.
    const hostDeleted = new HostMessage(
      B.MessageData.create({
        ...boundMessage(custom).data,
        content: boundMessage(deletedText).data.content,
      }),
      session,
    );
    if (hostDeleted.content.tag !== B.MessageContent_Tags.DeletedMessage)
      throw new Error("the host lifted deleted custom bytes as content");
  } catch (error) {
    const detail =
      error instanceof sdk.XmtpError ? error.details.message : undefined;
    throw new Error(`${step}: ${String(error)}: ${String(detail)}`);
  } finally {
    await client?.end();
    worker.terminate();
  }
}
