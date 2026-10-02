import { type Client } from "@xmtp/node-sdk";

export async function syncConversations(client: Client) {
  const consentStates = ["allowed" as const];
  // #region sync
  const summary = await client.conversations.syncAll(consentStates);
  // #endregion sync
  return summary;
}
