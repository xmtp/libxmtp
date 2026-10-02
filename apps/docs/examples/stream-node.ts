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
  await stream.ready();
  void stream.onValue(handleConversation).catch(console.error);
  // #endregion stream
  return stream;
}
