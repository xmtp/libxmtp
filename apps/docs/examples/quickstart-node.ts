// #region client
import {
  Client,
  ConsentState,
  IdentifierKind,
  type Signer,
} from "@xmtp/node-sdk";
import { hexToBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

// Use a new wallet and an in-memory database for this local test.
const account = privateKeyToAccount(generatePrivateKey());
const signer: Signer = {
  type: "EOA",
  getIdentifier: () => ({
    identifier: account.address,
    identifierKind: IdentifierKind.Ethereum,
  }),
  signMessage: async (message) =>
    hexToBytes(await account.signMessage({ message })),
};
const client = await Client.create(signer, {
  backendUrl: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050",
  dbEncryptionKey: crypto.getRandomValues(new Uint8Array(32)),
  dbPath: null,
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
const stream = await client.conversations.streamAllMessages({
  onValue: (message) => console.log("New message:", message),
  onError: console.error,
});
await client.conversations.syncAll([ConsentState.Allowed]);
// #endregion stream

export { client, stream };
