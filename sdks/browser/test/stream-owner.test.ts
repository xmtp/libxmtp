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
import { create, options, signer } from "./helpers";
const makeClient = (codec: ContentCodec<string>) =>
  create(signer(), { codecs: [codec] });

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
async function createWorkerClient(contentCodec: ContentCodec<string>) {
  const worker = new Worker(
    new URL("../dist/typescript-wasm/worker-entry.gen.js", import.meta.url),
    { type: "module" },
  );
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
  const peer = await makeClient(codec("peer"));
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
    const owner = streamOwner(raw);
    expect(owner).toBeDefined();
    expect(publicClient(owner!)).toBe(client);
    const streamed = (await next).value!;
    expect(streamed.id).toBe(created.id);
    await notifications.end();
    const dm = await client.conversations.createDm(peer.inboxId);
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
    const dmId = await dm.send(codec(label).encode("dm"));
    const common = (await client.conversations.getById(dm.id))!;
    const stream = common.streamMessages({ from });
    try {
      let item = (await stream.next()).value!;
      while (item.id !== dmId) item = (await stream.next()).value!;
      expect(item.id).toBe(dmId);
      expect(item.content).toMatchObject({
        kind: "custom",
        value: `${label}:dm`,
      });
    } finally {
      await stream.end();
    }
    const all = client.conversations.streamAllMessages({
      from,
      conversationKind: "dm",
    });
    try {
      let item = (await all.next()).value!;
      while (item.content.kind !== "custom") item = (await all.next()).value!;
      expect(item.id).toBe(dmId);
    } finally {
      await all.end();
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

// verifies: CONS-030, CONS-042, CONS-043, CONS-044
it("stream options preserve consent defaults, empty selection, and the replay cursor", async () => {
  const client = await makeClient(codec("selection"));
  const allowed = await client.conversations.createGroup([]);
  const unknown = await client.conversations.createGroup([]);
  const denied = await client.conversations.createGroup([]);
  await allowed.updateConsentState("allowed");
  await unknown.updateConsentState("unknown");
  await denied.updateConsentState("denied");
  for (const group of [allowed, unknown, denied])
    await group.sendText("before cursor");
  const { cursor: from } =
    await client.conversations.messageHistorySnapshot(100);
  const allowedId = await allowed.sendText("allowed after cursor");
  const unknownId = await unknown.sendText("unknown after cursor");
  const deniedId = await denied.sendText("denied after cursor");
  // Sending allows a conversation. Set the fixture states after sending.
  await unknown.updateConsentState("unknown");
  await denied.updateConsentState("denied");
  expect((await allowed.state()).common.consentState).toBe("allowed");
  expect((await unknown.state()).common.consentState).toBe("unknown");
  expect((await denied.state()).common.consentState).toBe("denied");
  for (const [consentStates, ids] of [
    [undefined, [allowedId, unknownId]],
    [["allowed"], [allowedId]],
    [["denied"], [deniedId]],
  ] as const) {
    const stream = client.conversations.streamAllMessages({
      from,
      consentStates,
    });
    try {
      const received: string[] = [];
      for (const _id of ids) received.push((await stream.next()).value!.id);
      expect(received).toEqual(ids);
    } finally {
      await stream.end();
    }
  }
  const empty = client.conversations.streamAllMessages({
    from,
    consentStates: [],
  });
  try {
    await empty.ready();
    const pending = empty.next();
    expect(
      await Promise.race([
        pending.then(() => "delivered"),
        new Promise<string>((resolve) =>
          setTimeout(() => resolve("waiting"), 250),
        ),
      ]),
    ).toBe("waiting");
    await empty.end();
    expect((await pending).done).toBe(true);
  } finally {
    await empty.end();
  }
});
