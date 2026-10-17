import type { Client as NodeClient } from "@xmtp/node-sdk";

export async function createAgentGroup(
  organizer: NodeClient,
  instinctInboxId: string,
  museInboxId: string,
  grokbotInboxId: string,
  claudeInboxId: string,
  codexInboxId: string,
) {
  // #region group
  const group = await organizer.conversations.createGroup([
    instinctInboxId,
    museInboxId,
    grokbotInboxId,
    claudeInboxId,
    codexInboxId,
  ]);

  await group.sendText("Let us work on this together.");
  // #endregion group
  return group;
}
