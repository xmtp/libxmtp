import type { Client, Member } from "@xmtp/node-sdk";

import { handleFields } from "@/utils/handles";

/**
 * Add cached installation IDs to conversation member output. Each row has
 * the member's current handle. With `withOtherHandles` (the JSON output),
 * each row also has the member's other handles.
 */
export async function memberDetails(
  client: Pick<Client, "inboxStates">,
  members: Member[],
  withOtherHandles: boolean,
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
    const { handle, otherHandles } = handleFields(member);
    return {
      inboxId: member.inboxId,
      handle,
      ...(withOtherHandles ? { otherHandles } : {}),
      accountIdentifiers: member.identities,
      installationIds,
      permissionLevel: member.permissionLevel,
      consentState: member.consentState,
    };
  });
}
