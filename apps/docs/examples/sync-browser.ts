import type { Client, ConsentState } from "@xmtp/browser-sdk";

export async function syncConversations(
  client: Client,
  consentStates: ConsentState[],
) {
  // #region sync
  const summary = await client.conversations.syncAll(consentStates);
  // #endregion sync
  return summary;
}
