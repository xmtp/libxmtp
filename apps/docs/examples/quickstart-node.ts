// #region client
import { Client, generateLocalSigner } from "@xmtp/node-sdk";

// Use a new signer and an in-memory database for this local test.
const signer = await generateLocalSigner();
const client = await Client.create(signer, {
  backend: { url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050" },
  storage: {
    location: "inMemory",
    encryptionKey: crypto.getRandomValues(new Uint8Array(32)),
  },
});
console.log("Your inbox ID:", client.inboxId);
// #endregion client

// #region send
const recipientInboxId = process.env.XMTP_RECIPIENT_INBOX_ID;
if (!recipientInboxId) throw new Error("Set XMTP_RECIPIENT_INBOX_ID");
const group = await client.conversations.createGroup([recipientInboxId]);
await group.sendText("Hello everyone");
// #endregion send

// #region stream
const stream = client.conversations.streamAllMessages({
  consentStates: ["allowed"],
});
const receive = (async () => {
  for await (const message of stream) console.log("New message:", message);
})();
await client.conversations.syncAll(["allowed"]);
// Call stream.return() and await receive when you stop the stream.
// #endregion stream

export { client, stream, receive };
