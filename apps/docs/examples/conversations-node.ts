import type { Client } from "@xmtp/node-sdk";

export async function createConversation(
  client: Client,
  memberInboxId: string,
) {
  // #region create
  const group = client.conversations.createGroupOptimistic();
  await group.addMembers([memberInboxId]);
  // #endregion create
  return group;
}
