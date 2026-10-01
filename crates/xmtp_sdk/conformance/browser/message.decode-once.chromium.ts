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
import { expect, options, signer } from "./suite-support";

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
    await sdk.initPureWasm();
    await session.ready();
    const projection = currentProjection();
    const configured: sdk.ClientOptions = {
      ...options(`decode-once-${crypto.randomUUID()}.db`, backendURL),
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
    const group = await client.conversations.createGroup([]);
    const parentId = await group.sendText("parent");
    const textId = await group.sendText(text, { compression: "gzip" });
    const replyId = await client.conversations.replyToMessage(
      parentId,
      new sdk.TextCodec().encode(text),
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
      expect(
        (await session.call("__textDecodeCount", [])) === 1n,
        "standard content decoded more than once",
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
    custom.calls = 0;
    const reply = await client.conversations.getMessageById(customReplyId);
    expect(custom.calls === 1, "nested custom body did not decode once");
    expect(
      reply?.replyContent?.kind === "custom" &&
        reply.replyContent.value === "custom decoded",
      "custom reply value was lost",
    );
    expect(
      reply?.content.kind === "reply" &&
        reply.content.body.kind === "custom" &&
        reply.content.body.value === "custom decoded",
      "public reply body lost its custom value",
    );
    expect(
      reply?.encoded !== undefined &&
        reply.replyContent?.kind === "custom" &&
        reply.replyContent.rawBytes.toString() ===
          reply.encoded.content.toString(),
      "nested raw bytes changed",
    );
    custom.fail = true;
    custom.calls = 0;
    const failed = await client.conversations.getMessageById(customReplyId);
    expect(custom.calls === 1, "failed nested custom body did not decode once");
    expect(
      failed?.replyContent?.kind === "custom" &&
        failed.replyContent.error?.code === "CodecDecodeFailed" &&
        failed.replyContent.encoded.content.toString() === "7,8,9",
      "nested custom failure evidence was lost",
    );
    expect(
      failed?.content.kind === "reply" &&
        failed.content.body.kind === "custom" &&
        failed.content.body.error?.code === "CodecDecodeFailed",
      "public reply lost the custom failure",
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
      enriched?.reactions.some(
        (reaction) =>
          reaction.id === reactionId && reaction.reaction.content === "👍",
      ),
      "reaction enrichment was lost",
    );
  } finally {
    await client?.end();
    worker.terminate();
  }
}
