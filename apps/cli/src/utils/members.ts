import type { Client, Member } from "@xmtp/node-sdk";

/** Add cached installation IDs to conversation member output. */
export async function memberDetails(
  client: Pick<Client, "inboxStates">,
  members: Member[],
) {
  if (members.length === 0) return [];

  const inboxIds = [...new Set(members.map((member) => member.inboxId))];
  const states = await client.inboxStates(inboxIds, false);
  const installationsByInboxId = new Map(
    states.map((state) => [
      state.inboxId,
      state.installations.map((installation) => installation.id),
    ]),
  );

  return members.map((member) => {
    const installationIds = installationsByInboxId.get(member.inboxId);
    if (!installationIds)
      throw new Error(`Inbox state not found: ${member.inboxId}`);
    return {
      inboxId: member.inboxId,
      accountIdentifiers: member.identities,
      installationIds,
      permissionLevel: member.permissionLevel,
      consentState: member.consentState,
    };
  });
}
