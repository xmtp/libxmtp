import {
  type Conversation,
  type Message as XmtpMessage,
} from "@xmtp/browser-sdk";

/**
 * Returns the most recent conversation creation timestamp.
 * Used to track the latest conversation when syncing from the network and
 * sorting conversations.
 */
export const getLastCreatedAt = (
  conversation: Conversation,
  lastCreatedAt?: bigint,
) => {
  return !lastCreatedAt ||
    (conversation.createdAt.ns && conversation.createdAt.ns > lastCreatedAt)
    ? conversation.createdAt.ns
    : lastCreatedAt;
};

/**
 * Checks if a message was sent after the last sent timestamp.
 * Used to track the latest message when syncing from the network and
 * determining sort order of conversations and messages.
 */
export const isLastSentAt = (message: XmtpMessage, lastSentAt?: bigint) => {
  return !lastSentAt || message.sentAt.ns > lastSentAt;
};

/**
 * Sorts conversations by most recent activity (last message or creation time).
 * Conversations with more recent messages appear first.
 */
export const sortConversations = (
  conversations: Map<string, Conversation>,
  lastMessages: Map<string, XmtpMessage | undefined>,
) => {
  const sortedConversations = Array.from(conversations.values()).sort(
    (a, b) => {
      const aLastMessage = lastMessages.get(a.id);
      const bLastMessage = lastMessages.get(b.id);
      const aVal = aLastMessage?.sentAt.ns ?? a.createdAt.ns;
      const bVal = bLastMessage?.sentAt.ns ?? b.createdAt.ns;
      return Number(bVal - aVal);
    },
  );
  return sortedConversations;
};

/**
 * Sorts messages by sent time in ascending order (oldest first).
 * Used to display messages in chronological order within a conversation.
 */
export const sortMessages = (messages: Map<string, XmtpMessage>) => {
  const sortedMessages = Array.from(messages.values()).sort((a, b) => {
    return Number(a.sentAt.ns - b.sentAt.ns);
  });
  return sortedMessages;
};
