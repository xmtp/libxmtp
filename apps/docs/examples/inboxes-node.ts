import { Client, type BackendSource, type Signer } from "@xmtp/node-sdk";
export async function manageInboxes(
  client: Client,
  signer: Signer,
  inboxIds: string[],
  backend: BackendSource,
  installationIds: string[],
) {
  // #region manage
  const state = await client.inboxState(false);
  const states = await Client.inboxStates(inboxIds, backend);
  await client.revokeInstallations(signer, installationIds);
  await client.revokeAllOtherInstallations(signer);
  // #endregion manage
  return { state, states };
}
