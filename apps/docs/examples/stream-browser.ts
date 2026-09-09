import type { Client, Conversation } from "@xmtp/browser-sdk";

export async function streamConversations(
  client: Client,
  handleConversation: (conversation: Conversation | undefined) => void,
) {
  // #region stream
  const stream = await client.conversations.stream({
    onValue: handleConversation,
  });
  // #endregion stream
  return stream;
}
