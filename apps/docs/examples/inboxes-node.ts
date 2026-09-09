import { Client, type Backend } from "@xmtp/node-sdk";

export async function manageInboxes(
  client: Client,
  inboxIds: string[],
  backend: Backend,
  installationIds: Uint8Array[],
) {
  // #region manage
  const state = await client.preferences.fetchInboxState();
  const states = await Client.fetchInboxStates(inboxIds, backend);
  await client.revokeInstallations(installationIds);
  await client.revokeAllOtherInstallations();
  // #endregion manage
  return { state, states };
}
