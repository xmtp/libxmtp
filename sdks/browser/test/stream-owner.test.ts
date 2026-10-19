import { XmtpError, type ContentCodec } from "@xmtp/browser-sdk";
import { afterEach, expect, test as it } from "vitest";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../dist/typescript-wasm/contract.gen";
import { Client as ProxyClient } from "../dist/typescript-wasm/proxy.gen";
import { wrapClient } from "../dist/typescript-wasm/public-client.gen";
import {
  currentProjection,
  lowerSigner,
} from "../dist/typescript-wasm/public-values.gen";
import { MainSession } from "../dist/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../dist/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../dist/typescript-wasm/runtime/public/client";
import { options, signer } from "./helpers";

// Rust and the Node streamOwner test cover stream owners on a live client
// (sdks/node/test/streamOwner.integration.test.ts). This case is worker-only:
// a failed remote shutdown must still close the stream methods.

const workerClients: {
  client: ReturnType<typeof publicClient>;
  session: MainSession;
  worker: Worker;
}[] = [];
afterEach(async () => {
  for (const { client, session, worker } of workerClients.splice(0)) {
    try {
      await client.end();
    } finally {
      session.terminate();
      worker.terminate();
    }
  }
});
async function createWorkerClient(
  contentCodec: ContentCodec<string>,
  worker = new Worker(
    new URL("../dist/typescript-wasm/worker-entry.gen.js", import.meta.url),
    { type: "module" },
  ),
) {
  const lifetimeLock = `xmtp-worker:${crypto.randomUUID()}`;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(
        message.t === "hello" ? { ...message, lifetimeLock } : message,
        { transfer },
      );
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
  const projection = currentProjection();
  const proxy = await ProxyClient.create(
    session,
    lowerSigner(signer(), projection),
    hostOptions({ ...options, codecs: [contentCodec] }, projection),
  );
  const client = publicClient(wrapClient(proxy));
  workerClients.push({ client, session, worker });
  return client;
}

const type = {
  authorityId: "tests.xmtp.org",
  typeId: "stream-owner",
  versionMajor: 1,
  versionMinor: 0,
};
function codec(label: string): ContentCodec<string> {
  return {
    type,
    encode: (value) => ({
      type,
      parameters: new Map(),
      content: new TextEncoder().encode(value),
    }),
    decode: (encoded) =>
      `${label}:${new TextDecoder().decode(encoded.content)}`,
  };
}

it("rejects a stream owner after remote shutdown reports a failure", async () => {
  const client = await createWorkerClient(
    codec("shutdown"),
    new Worker(
      new URL("./fixtures/failed-shutdown-worker.ts", import.meta.url),
      {
        type: "module",
      },
    ),
  );
  const conversations = client.conversations;
  const session = workerClients.at(-1)!.session;
  const call = session.call.bind(session);
  let shutdownCalls = 0;
  let shutdownReplies = 0;
  session.call = async (...args) => {
    if (args[0] === "Client.end") shutdownCalls++;
    const result = await call(...args);
    if (args[0] === "Client.end") shutdownReplies++;
    return result;
  };
  try {
    await expect(client.end()).rejects.toThrow();
    expect(() => conversations.stream()).toThrow(XmtpError.ClientClosed);
    expect(() => conversations.streamAllMessages()).toThrow(
      XmtpError.ClientClosed,
    );
    await client.end();
    expect(shutdownCalls).toBe(2);
    expect(shutdownReplies).toBe(1);
  } finally {
    session.call = call;
    await client.end();
  }
});
