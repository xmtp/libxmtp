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
  unwrapGroup,
} from "../dist/typescript-wasm/public-values.gen";
import {
  RemoteObject,
  sessionOf,
} from "../dist/typescript-wasm/runtime/bridge/main/remote-object";
import { MainSession } from "../dist/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../dist/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../dist/typescript-wasm/runtime/public/client";
import { streamOwner } from "../dist/typescript-wasm/runtime/public/host";
import { options, signer } from "./helpers";

// The browser host keys stream owners by worker session and then by client
// key. Each worker numbers its clients from 1, so two sessions can hold the
// same key. Node keys owners only by client key and cannot cover this.

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

// verifies: CTYPE-008, CTYPE-009, PROC-034
it("stream methods use the receiver owner for create, get, list, and streamed values", async () => {
  const first = await createWorkerClient(codec("first"));
  const second = await createWorkerClient(codec("second"));
  let firstOwnerKey: bigint | undefined;
  let firstSession: MainSession | undefined;
  for (const [client, label] of [
    [first, "first"],
    [second, "second"],
  ] as const) {
    const notifications = client.conversations.stream();
    await notifications.ready();
    const next = notifications.next();
    const created = await client.conversations.createGroup([]);
    const raw = unwrapGroup(created);
    const ownerKey = raw.sdkStreamOwnerKey();
    if (!(raw instanceof RemoteObject))
      throw new Error("The receiver is not a worker proxy");
    const session = sessionOf(raw);
    if (firstOwnerKey === undefined) {
      firstOwnerKey = ownerKey;
      firstSession = session;
    } else {
      expect(ownerKey).toBe(firstOwnerKey);
      expect(session).not.toBe(firstSession);
    }
    const owner = streamOwner(raw, () => raw.sdkStreamOwnerKey());
    expect(owner).toBeDefined();
    expect(publicClient(owner!)).toBe(client);
    const streamed = (await next).value!;
    expect(streamed.id).toBe(created.id);
    await notifications.end();
    const found = (await client.conversations.getById(created.id))!;
    const listed = (await client.conversations.list()).find(
      (value) => value.id === created.id,
    )!;
    const from = await client.conversations.beginningDeliveryCursor();
    const id = await created.send(codec(label).encode("group"));
    for (const receiver of [created, found, listed, streamed]) {
      const stream = receiver.streamMessages({ from });
      try {
        const item = (await stream.next()).value!;
        expect(item.id).toBe(id);
        expect(item.content).toMatchObject({
          kind: "custom",
          value: `${label}:group`,
        });
      } finally {
        await stream.end();
      }
    }
  }
  const detached = await first.conversations.createGroup([]);
  await first.end();
  expect(() => detached.streamMessages()).toThrow(XmtpError.ClientClosed);
  expect(() => first.conversations.stream()).toThrow(XmtpError.ClientClosed);
  expect(() => first.conversations.streamAllMessages()).toThrow(
    XmtpError.ClientClosed,
  );
  const live = await second.conversations.createGroup([]);
  const stream = live.streamMessages();
  try {
    const id = await live.send(codec("second").encode("still live"));
    expect((await stream.next()).value).toMatchObject({
      id,
      content: { kind: "custom", value: "second:still live" },
    });
  } finally {
    await stream.end();
  }
});

// A failed remote shutdown must still close the stream methods.
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
