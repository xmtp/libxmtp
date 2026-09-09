import {
  MessageSortBy,
  SortDirection,
  type Client,
  type Conversation,
  type Group,
} from "@xmtp/browser-sdk";

export async function readMessages(conversation: Conversation) {
  // #region messages
  const messages = await conversation.messages({ limit: 10n });
  // #endregion messages
  return messages;
}

export async function paginateMessages(group: Group) {
  // #region pagination
  const firstPage = await group.messages({
    limit: 20n,
    sortBy: MessageSortBy.SentAt,
    direction: SortDirection.Descending,
  });
  const secondPage = await group.messages({
    limit: 20n,
    sortBy: MessageSortBy.SentAt,
    direction: SortDirection.Descending,
    sentBeforeNs: firstPage.at(-1)?.sentAtNs,
  });
  // #endregion pagination
  return secondPage;
}

export async function listConversations(client: Client) {
  // #region conversations
  const conversations = await client.conversations.list();
  // #endregion conversations
  return conversations;
}
