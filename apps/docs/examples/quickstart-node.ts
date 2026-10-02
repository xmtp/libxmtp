// #region client
import { Client, MessageStream, type Signer } from "@xmtp/node-sdk";
import { hexToBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
const account = privateKeyToAccount(generatePrivateKey());
const signer: Signer = {
  identity: async () => ({ identifier: account.address, kind: "ethereum" }),
  kind: async () => ({ kind: "eoa" }),
  sign: async (request) => ({
    kind: "ecdsa",
    value: hexToBytes(await account.signMessage({ message: request.text })),
  }),
};
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
const stream = MessageStream.open(client);
await stream.ready();
void stream
  .onValue((message) => console.log("New message:", message))
  .catch(console.error);
await client.conversations.syncAll(["allowed"]);
// #endregion stream
export { client, stream };
