import assert from "node:assert/strict";

import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import {
  PoolLocks,
  WorkerHost,
  type LockProvider,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.ts";

if (typeof global.gc !== "function")
  throw new Error("run this proof with --expose-gc");

const sent: WireMessage[] = [];
let receive: (message: WireMessage) => void = () => {};
const endpoint: WireEndpoint = {
  postMessage(message) {
    sent.push(message);
    if (message.t === "hello")
      queueMicrotask(() => receive({ t: "ready", epoch: 1 }));
    if (message.t === "call" && message.key === "Conversations.createGroup")
      queueMicrotask(() =>
        receive({
          t: "return",
          id: message.id,
          value: {
            h: 5,
            owner: 1,
            epoch: 1,
            type: "Group",
            snap: { id: "group" },
          },
        }),
      );
  },
  onMessage(handler) {
    receive = handler;
  },
  onExit() {},
};
const session = new MainSession(endpoint, 1, "gc");
await session.ready();
const client = new Client(session, {
  h: 1,
  owner: 1,
  epoch: 1,
  type: "Client",
  snap: {
    conversations: { h: 2, owner: 1, epoch: 1, type: "Conversations" },
    inboxID: "inbox",
    installationID: "install",
  },
});
assert.strictEqual(
  client.conversations(),
  client.conversations(),
  "same handle must return one proxy",
);
for (let attempt = 0; attempt < 30; attempt++) {
  global.gc();
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
}
assert.ok(
  !sent.some(
    (message) => message.t === "release" && message.handles.includes(2),
  ),
  "live client must pin its snapshot handle",
);
assert.equal(client.conversations().handle.h, 2);
assert.equal((await client.conversations().createGroup([])).id(), "group");

const duplicateHandle = {
  h: 6,
  owner: 6,
  epoch: 1,
  type: "Client",
  snap: {
    conversations: { h: 7, owner: 6, epoch: 1, type: "Conversations" },
    inboxID: "x",
    installationID: "y",
  },
};
const first = new Client(session, duplicateHandle);
const second = new Client(session, duplicateHandle);
first.release();
await Promise.resolve();
assert.ok(
  !sent.some(
    (message) => message.t === "release" && message.handles.includes(6),
  ),
  "one live proxy must keep its handle",
);
second.release();
await Promise.resolve();
assert.ok(
  sent.some(
    (message) => message.t === "release" && message.handles.includes(6),
  ),
  "last proxy must release its handle",
);

function temporary(): void {
  const proxy = new Client(session, {
    h: 3,
    owner: 3,
    epoch: 1,
    type: "Client",
    snap: {
      conversations: { h: 4, owner: 3, epoch: 1, type: "Conversations" },
      inboxID: "x",
      installationID: "y",
    },
  });
  proxy.conversations();
}
temporary();
for (let attempt = 0; attempt < 100; attempt++) {
  global.gc();
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
  if (
    sent.some(
      (message) => message.t === "release" && message.handles.includes(3),
    )
  )
    break;
}
assert.ok(
  sent.some(
    (message) => message.t === "release" && message.handles.includes(3),
  ),
  "collected client did not release its handle",
);
assert.ok(
  sent.some(
    (message) => message.t === "release" && message.handles.includes(4),
  ),
  "collected snapshot did not release its handle",
);

const held = new Set<string>();
const provider: LockProvider = {
  async request(name, _options, callback) {
    if (held.has(name)) return callback(null);
    held.add(name);
    try {
      await callback({});
    } finally {
      held.delete(name);
    }
  },
};
let mainReceive: (message: WireMessage) => void = () => {};
let workerReceive: (message: WireMessage) => void = () => {};
const mainEndpoint: WireEndpoint = {
  postMessage(message) {
    queueMicrotask(() => workerReceive(structuredClone(message)));
  },
  onMessage(handler) {
    mainReceive = handler;
  },
  onExit() {},
};
const workerEndpoint: WireEndpoint = {
  postMessage(message) {
    queueMicrotask(() => mainReceive(structuredClone(message)));
  },
  onMessage(handler) {
    workerReceive = handler;
  },
  onExit() {},
};
const locks = new PoolLocks(provider);
const otherTab = new PoolLocks(provider);
const host = new WorkerHost(
  workerEndpoint,
  1,
  "gc-pool",
  async () => {},
  async () => undefined,
  locks,
);
const lockSession = new MainSession(mainEndpoint, 1, "gc-pool");
await lockSession.ready();
await locks.open("collected-client");
const lockHandle = host.registry.add({}, "Client");
locks.attachOwner(lockHandle.owner, "collected-client");
function temporaryLockedClient(): void {
  new Client(lockSession, lockHandle);
}
temporaryLockedClient();
await assert.rejects(otherTab.open("collected-client"), {
  code: "storageBusy",
});
for (let attempt = 0; attempt < 100 && host.registry.size > 0; attempt++) {
  global.gc();
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
}
assert.equal(host.registry.size, 0, "GC did not release the Client handle");
await otherTab.open("collected-client");
otherTab.close("collected-client");
console.log("proxy identity, snapshot pin, and finalizer release passed");
