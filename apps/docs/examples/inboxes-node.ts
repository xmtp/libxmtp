import { Client, type Backend, type Signer } from "@xmtp/node-sdk";

export async function manageInboxes(
  client: Client,
  inboxIds: string[],
  backend: Backend,
  installationIds: string[],
  signer: Signer,
) {
  // #region manage
  const state = await client.inboxState(true);
  const states = await Client.inboxStates(inboxIds, backend);
  await client.revokeInstallations(signer, installationIds);
  await client.revokeAllOtherInstallations(signer);
  // #endregion manage
  return { state, states };
}
