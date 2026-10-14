import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import * as sdk from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import { Client as ProxyClient } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/proxy.gen";
import { wrapClient } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-client.gen";
import {
  currentProjection,
  lowerSigner,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-values.gen";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/public/client";
import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import * as RawPure from "../../../../target/sdk-generated/typescript-pure/xmtp_sdk";
import nativePureModule from "../../../../target/sdk-generated/typescript-pure/xmtp_sdk-ffi";
import { expect, signer } from "./suite-support";

// A real worker counts calls at the Rust text decoder. A standard host codec
// throws if receive calls it. A custom reply body still calls its host codec.
export async function receivedStandardContentDecodesOnce(
  backendURL: string,
): Promise<void> {
  const worker = new Worker(
    new URL("./message.decode-once.worker.ts", import.meta.url),
    { type: "module" },
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
  let standardCalls = 0;
  const standalone = { calls: 0 };
  let restoreStandalone: (() => void) | undefined;
  const custom = { calls: 0, fail: false };
  const text = `decode-once-${crypto.randomUUID()}`;
  const customType: sdk.ContentTypeId = {
    authorityId: "decode.test",
    typeId: "custom",
    versionMajor: 1,
    versionMinor: 0,
  };
  const customEncoded: sdk.EncodedContent = {
    type: customType,
    parameters: new Map(),
    content: new Uint8Array([7, 8, 9]),
  };
  const poison = (typeId: string): sdk.ContentCodec<never> => ({
    type: { authorityId: "xmtp.org", typeId, versionMajor: 1, versionMinor: 0 },
    encode() {
      throw new Error("receive test codec cannot encode");
    },
    decode() {
      standardCalls++;
      throw new Error("second standard decode");
    },
  });
  try {
    await Pure.initPureWasm();
    // Guard the actual standalone decoder used by the host. Registry codecs
    // do not detect a direct call to Pure.decodeStandard.
    const native = nativePureModule();
    const entry = "uniffi_xmtp_sdk_fn_func_decode_standard";
    const decode = Object.getOwnPropertyDescriptor(native, entry);
    expect(decode !== undefined, "standalone decoder entry was not found");
    Object.defineProperty(native, entry, {
      ...decode,
      value: () => {
        standalone.calls++;
        throw new Error("second standard host decode");
      },
    });
    restoreStandalone = () => {
      Object.defineProperty(native, entry, decode);
    };
    let guardError: unknown;
    try {
      RawPure.decodeStandard(RawPure.encodeText("host decoder guard"));
    } catch (error) {
      guardError = error;
    }
    expect(
      guardError instanceof Error &&
        guardError.message === "second standard host decode" &&
        standalone.calls === 1,
      "standalone host decoder guard was not armed",
    );
    standalone.calls = 0;
    console.log("actual standalone host decoder guard is armed");
    await session.ready();
    const projection = currentProjection();
    const path = `decode-once-${crypto.randomUUID()}.db`;
    const configured: sdk.ClientOptions = {
      backend: { url: backendURL },
      storage: {
        location: { dbPath: path, attachmentsDir: `${path}-attachments` },
        label: path,
        singleConnection: false,
      },
      deviceSync: false,
      allowOffline: false,
      registration: { auto: true },
      codecs: [
        poison("text"),
        poison("reply"),
        {
          type: customType,
          encode() {
            return customEncoded;
          },
          decode(encoded) {
            custom.calls++;
            if (custom.fail) throw new Error("custom decode failed");
            expect(
              encoded.content.toString() === "7,8,9",
              "custom bytes changed",
            );
            return "custom decoded";
          },
        },
      ],
    };
    const proxy = await ProxyClient.create(
      session,
      lowerSigner(signer(), projection),
      hostOptions(configured, projection),
    );
    client = publicClient(wrapClient(proxy));
    expect(client instanceof sdk.Client, "worker client was not public");
    const group = await client.conversations.createGroup([]);
    const parentId = await group.sendText("parent");
    const textId = await group.sendText(text, { compression: "gzip" });
    const replyId = await client.conversations.replyToMessage(
      parentId,
      new Pure.TextCodec().encode(text),
      { compression: "deflate" },
    );
    const customReplyId = await client.conversations.replyToMessage(
      parentId,
      customEncoded,
    );
    for (const id of [textId, replyId]) {
      await session.call("__watchTextDecode", [text]);
      const message = await client.conversations.getMessageById(id);
      const body = id === textId ? message?.content : message?.replyContent;
      expect(
        body?.kind === "text" && body.value === text,
        "worker text was lost",
      );
      const count = await session.call("__textDecodeCount", []);
      expect(
        count === 1n,
        `expected one standard decode, got ${String(count)}`,
      );
      console.log(
        `worker ${id === textId ? "text" : "reply"} decode count: ${String(count)}; standard host calls: ${standardCalls}`,
      );
      expect(standardCalls === 0, "receive used a standard host decoder");
      if (id === replyId) {
        expect(
          message?.inReplyToContent?.kind === "text" &&
            message.inReplyToContent.value === "parent",
          "reply parent was lost",
        );
        expect(
          (await message.parent())?.id === parentId,
          "parent action was lost",
        );
      }
    }
    await session.call("__watchTextDecode", ["parent"]);
    const withParent = await client.conversations.getMessageById(replyId);
    expect(
      withParent?.inReplyToContent?.kind === "text" &&
        withParent.inReplyToContent.value === "parent",
      "worker parent was lost",
    );
    const parentCount = await session.call("__textDecodeCount", []);
    expect(
      parentCount === 1n,
      `expected one parent decode, got ${String(parentCount)}`,
    );
    console.log(
      `worker parent decode count: ${String(parentCount)}; standard host calls: ${standardCalls}`,
    );
    expect(
      standardCalls === 0,
      "receive used a standard host decoder for the parent",
    );
    custom.calls = 0;
    const reply = await client.conversations.getMessageById(customReplyId);
    expect(custom.calls === 1, "nested custom body did not decode once");
    expect(
      reply?.replyContent?.kind === "custom" &&
        reply.replyContent.value === "custom decoded",
      "custom reply value was lost",
    );
    expect(
      reply.content.kind === "reply" &&
        reply.content.body.kind === "custom" &&
        reply.content.body.value === "custom decoded",
      "public reply body lost its custom value",
    );
    expect(
      reply.encoded !== undefined &&
        reply.replyContent.rawBytes.toString() ===
          reply.encoded.content.toString(),
      "nested raw bytes changed",
    );
    custom.fail = true;
    custom.calls = 0;
    const failed = await client.conversations.getMessageById(customReplyId);
    expect(custom.calls === 1, "failed nested custom body did not decode once");
    expect(
      failed?.content.kind === "unknown" &&
        failed.content.error.code === "CodecDecodeFailed" &&
        failed.content.error.category === "callback" &&
        failed.content.error.retryable === false,
      "outer reply did not keep the custom failure",
    );
    expect(
      failed.replyContent === undefined,
      "failed reply exposed a decoded body",
    );
    expect(
      failed.content.rawBytes.toString() === reply.rawBytes.toString() &&
        failed.rawBytes.toString() === reply.rawBytes.toString(),
      "failed outer reply bytes changed",
    );
    expect(
      failed.encoded?.content.toString() === reply.encoded.content.toString(),
      "failed outer reply envelope changed",
    );
    const parent = await reply.parent();
    expect(parent?.id === parentId, "parent action was lost");
    const reactionId = await parent.react({
      content: "👍",
      action: "added",
      schema: "unicode",
    });
    const enriched = await parent.refresh();
    expect(enriched?.replyCount === 2n, "reply count enrichment was lost");
    expect(
      enriched.reactions.some(
        (reaction) =>
          reaction.id === reactionId && reaction.reaction.content === "👍",
      ),
      "reaction enrichment was lost",
    );
    custom.fail = false;
    const customParentId = await group.send(customEncoded);
    const customParent =
      await client.conversations.getMessageById(customParentId);
    expect(customParent?.content.kind === "custom", "custom parent was lost");
    const parentFailureReplyId = await client.conversations.replyToMessage(
      customParentId,
      new Pure.TextCodec().encode("valid child"),
    );
    custom.fail = true;
    custom.calls = 0;
    const parentFailed =
      await client.conversations.getMessageById(parentFailureReplyId);
    expect(custom.calls === 1, "failed custom parent did not decode once");
    expect(
      parentFailed?.content.kind === "reply" &&
        parentFailed.replyContent?.kind === "text" &&
        parentFailed.replyContent.value === "valid child",
      "parent codec failure changed the child",
    );
    expect(
      parentFailed.inReplyToContent?.kind === "custom" &&
        parentFailed.inReplyToContent.error?.code === "CodecDecodeFailed" &&
        parentFailed.inReplyToContent.error.category === "callback",
      "parent codec failure was lost",
    );
    expect(
      parentFailed.inReplyTo?.rawBytes.toString() ===
        customParent.rawBytes.toString() &&
        parentFailed.inReplyToContent.rawBytes.toString() ===
          customParent.rawBytes.toString(),
      "failed parent bytes changed",
    );
    await client.conversations.deleteMessage(textId);
    await group.sync();
    await session.call("__watchTextDecode", [text]);
    const deleted = await client.conversations.getMessageById(textId);
    expect(
      deleted?.content.kind === "deletedMessage",
      "deleted text was decoded as content",
    );
    expect(
      deleted.rawBytes.length === 0 && deleted.encoded?.content.length === 0,
      "deleted text bytes were exposed",
    );
    expect(standalone.calls === 0, "receive used the standalone host decoder");
  } finally {
    restoreStandalone?.();
    await client?.end();
    worker.terminate();
  }
}
