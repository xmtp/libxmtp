import assert from "node:assert/strict";

import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";

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
console.log("proxy identity, snapshot pin, and finalizer release passed");
