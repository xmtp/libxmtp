// #region app
import { Client } from "@xmtp/browser-sdk";

const log = (line: string) => {
  document.body.append(line, document.createElement("br"));
};

// A new identity and an in-memory database for each page load.
const client = await Client.create(await Client.generateKeys(), {
  backend: { url: "http://127.0.0.1:5050" },
  storage: { location: "inMemory" },
});
log(`Your inbox ID: ${client.inboxId}`);
log(`To message this tab, open ${location.origin}/?to=${client.inboxId}`);

// With ?to=<inbox-id>, send one message to that inbox.
const to = new URLSearchParams(location.search).get("to");
if (to) {
  const dm = await client.conversations.createDm(to);
  await dm.sendText("Hello from the browser");
}

// The stream also finds new conversations.
for await (const message of client.conversations.streamAllMessages()) {
  if (message.content.kind === "text") {
    const sender = message.senderInboxId === client.inboxId ? "You" : "Them";
    log(`${sender}: ${message.content.value}`);
  }
}
// #endregion app
