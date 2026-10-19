// #region send
import { Client } from "@xmtp/node-sdk";

const [inboxId, text = "Hello from XMTP"] = process.argv.slice(2);
if (!inboxId) throw new Error("Usage: node send.ts <inbox-id> [message]");

const client = await Client.create(await Client.generateKeys(), {
  backend: { url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050" },
  storage: { location: "inMemory" },
});
const dm = await client.conversations.createDm(inboxId);
await dm.sendText(text);
console.log(`Sent: ${text}`);
await client.end();
// #endregion send
