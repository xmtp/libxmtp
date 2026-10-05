import {
  ConversationStream,
  type Client,
  type Conversation,
} from "@xmtp/node-sdk";

export async function streamConversations(
  client: Client,
  handleConversation: (conversation: Conversation) => void,
) {
  // #region stream
  const stream = ConversationStream.open(client);
  const receive = (async () => {
    for await (const conversation of stream) handleConversation(conversation);
  })();
  // #endregion stream
  return { stream, receive };
}
