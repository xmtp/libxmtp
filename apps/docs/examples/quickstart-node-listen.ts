// #region listen
import { Client } from "@xmtp/node-sdk";

// A new identity and an in-memory database. Both are gone when the script stops.
const client = await Client.create(await Client.generateKeys(), {
  backend: { url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050" },
  storage: { location: "inMemory" },
});
console.log(
  `Listening. In another terminal, run:\n\n  node send.ts ${client.inboxId}\n`,
);

// The stream also finds new conversations. Press Ctrl+C to stop.
for await (const message of client.conversations.streamAllMessages()) {
  if (message.content.kind === "text") {
    console.log(`New message: ${message.content.value}`);
  }
}
// #endregion listen
