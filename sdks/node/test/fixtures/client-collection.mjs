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
console.log("PASS client collection");
process.exit(0);
