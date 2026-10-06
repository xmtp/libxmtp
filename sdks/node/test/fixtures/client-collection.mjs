// Run with --expose-gc. The SDK holds a client only weakly: a dropped client
// is collected, and its messages then fail with ClientClosed.
import assert from "node:assert/strict";

import { Client, XmtpError, generateLocalSigner } from "@xmtp/node-sdk";

let message;
const weak = await (async () => {
  const client = await Client.create(await generateLocalSigner(), {
    backend: { url: process.env.XMTP_BACKEND_URL },
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  const group = await client.conversations.createGroup([]);
  const id = await group.sendText("weak owner");
  message = (await group.messages()).find((item) => item.id === id);
  assert.equal(message.client(), client);
  return new WeakRef(client);
})();
const tick = () => new Promise((resolve) => setTimeout(resolve, 20));
// deref() keeps its target alive until the current job ends, so collect in a
// new job and read the reference only after it.
for (let attempt = 0; attempt < 50; attempt++) {
  await tick();
  globalThis.gc();
  await tick();
  if (weak.deref() === undefined) break;
}
assert.equal(weak.deref(), undefined, "the SDK kept a dropped client alive");
assert.throws(
  () => message.client(),
  (error) => error instanceof XmtpError.ClientClosed,
);
// Keep a second client alive. A wrong-owner lookup must not pass merely because
// the receiver can still read its native scope while both hosts are retained.
const otherClient = await Client.create(await generateLocalSigner(), {
  backend: { url: process.env.XMTP_BACKEND_URL },
  storage: { location: "inMemory" },
  deviceSync: false,
});
// A stream keeps its own host alive even if the app retains only its receiver.
let stream;
let receiver;
const streamOwner = await (async () => {
  const client = await Client.create(await generateLocalSigner(), {
    backend: { url: process.env.XMTP_BACKEND_URL },
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  receiver = await client.conversations.createGroup([]);
  await receiver.sendText("stream owner");
  stream = receiver.streamMessages();
  return new WeakRef(client);
})();
await tick();
globalThis.gc();
await tick();
assert.notEqual(
  streamOwner.deref(),
  undefined,
  "a stream lost its host before consumption",
);
assert.equal((await stream.next()).value.content.value, "stream owner");
await tick();
globalThis.gc();
await tick();
assert.notEqual(
  streamOwner.deref(),
  undefined,
  "an active stream lost its host",
);
await stream.end();
stream = undefined;
for (let attempt = 0; attempt < 50; attempt++) {
  await tick();
  globalThis.gc();
  await tick();
  if (streamOwner.deref() === undefined) break;
}
assert.equal(
  streamOwner.deref(),
  undefined,
  "a released stream retained its host",
);
assert.throws(
  () => receiver.streamMessages(),
  (error) => error instanceof XmtpError.ClientClosed,
);
await otherClient.end();
console.log("PASS client collection");
process.exit(0);
