// #region client
import { Client, MessageStream, generateLocalSigner } from "@xmtp/browser-sdk";

// Use a new signer and an in-memory database for this local test.
const signer = await generateLocalSigner();
const client = await Client.create(signer, {
  backend: { url: "http://127.0.0.1:5050" },
  storage: { location: "inMemory" },
});
console.log("Your inbox ID:", client.inboxId);
// #endregion client

// #region send
const recipientInboxId = window.prompt("Recipient inbox ID");
if (!recipientInboxId) throw new Error("Enter a recipient inbox ID");
const group = await client.conversations.createGroup([recipientInboxId]);
await group.sendText("Hello everyone");
// #endregion send

// #region stream
const stream = MessageStream.open(client, { consentStates: ["allowed"] });
const receive = (async () => {
  for await (const message of stream) console.log("New message:", message);
})();
await client.conversations.syncAll(["allowed"]);
// Call stream.return() and await receive when you stop the stream.
// #endregion stream

export { client, stream, receive };
