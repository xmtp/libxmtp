import {
  type Client,
  type Conversation,
  type Group,
} from "@xmtp/react-native-sdk";

export async function readMessages(conversation: Conversation) {
  // #region messages
  const messages = await conversation.messages({ limit: 10 });
  // #endregion messages
  return messages;
}

export async function paginateMessages(group: Group) {
  // #region pagination
  const firstPage = await group.messageHistoryPage({
    limit: 20,
    direction: "descending",
  });
  if (!firstPage.hasMore || !firstPage.lastPosition) return [];
  // Keep the SDK position unchanged; it also covers unreadable rows.
  const secondPage = await group.messageHistoryPage(
    { limit: 20, direction: "descending" },
    firstPage.lastPosition,
  );
  // #endregion pagination
  return secondPage.messages;
}

export async function listConversations(client: Client) {
  // #region conversations
  const conversations = await client.conversations.list({
    consentStates: ["allowed"],
  });
  // #endregion conversations
  return conversations;
}
