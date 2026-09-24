import assert from "node:assert/strict";

import { RemoteObject } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/remote-object.ts";
import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire.ts";

if (typeof global.gc !== "function") {
  throw new Error("run this proof with --expose-gc");
}

const sent: WireMessage[] = [];
const endpoint: WireEndpoint = {
  postMessage(message) {
    sent.push(message);
  },
  onMessage() {},
  onExit() {},
};
const session = new MainSession(endpoint, 1, "gc");
function createProxy(): void {
  const proxy = new RemoteObject(session, {
    h: 1,
    owner: 1,
    epoch: 1,
    type: "Group",
  });
  assert.equal(proxy.handle.h, 1);
}
createProxy();
for (let attempt = 0; attempt < 100; attempt++) {
  global.gc();
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
  if (sent.some((message) => message.t === "release")) break;
}
assert.ok(
  sent.some((message) => message.t === "release" && message.handles[0] === 1),
  "collected proxy did not release its handle",
);
console.log("collected proxy released its worker handle");
