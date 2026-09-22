import type { Client as BrowserClient } from "@xmtp/browser-sdk";
import type { Client as NodeClient } from "@xmtp/node-sdk";

export async function createAgentGroup(
  organizer: NodeClient | BrowserClient,
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

  await group.sendText("Let’s work on this together.");
  // #endregion group
  return group;
}
