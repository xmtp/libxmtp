import type { Client as NodeClient } from "@xmtp/node-sdk";
import type { Client as BrowserClient } from "@xmtp/browser-sdk";

export async function createAgentGroup(
  client: NodeClient | BrowserClient,
  instinctInboxId: string,
  museInboxId: string,
  grokbotInboxId: string,
  claudeInboxId: string,
  codexInboxId: string,
  docInboxId: string,
) {
  // #region group
  const group = await client.conversations.createGroup([
    instinctInboxId,
    museInboxId,
    grokbotInboxId,
    claudeInboxId,
    codexInboxId,
    docInboxId,
  ]);

  await group.sendText("Let’s work on this together.");
  // #endregion group
  return group;
}
