import { type Client, type Conversation, type Group } from "@xmtp/browser-sdk";

export async function readMessages(conversation: Conversation) {
  // #region messages
  const messages = await conversation.messages({ limit: 10 });
  // #endregion messages
  return messages;
}

export async function paginateMessages(group: Group) {
  // #region pagination
  const firstPage = await group.messages({
    limit: 20,
    sortBy: "insertedAt",
    direction: "descending",
  });
  const secondPage = await group.messages({
    limit: 20,
    sortBy: "insertedAt",
    direction: "descending",
    insertedBefore: firstPage.at(-1)?.insertedAt,
  });
  // #endregion pagination
  return secondPage;
}

export async function listConversations(client: Client) {
  // #region conversations
  const conversations = await client.conversations.list({
    consentStates: ["allowed"],
  });
  // #endregion conversations
  return conversations;
}
