import type { ConsentState, Client } from "@xmtp/node-sdk";

export async function syncConversations(client: Client) {
  const consentStates: ConsentState[] = ["allowed"];
  // #region sync
  const summary = await client.conversations.syncAll(consentStates);
  // #endregion sync
  return summary;
}
