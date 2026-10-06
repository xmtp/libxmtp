import {
  ConversationStream,
  MessageStream,
  type Message,
  type Group,
  type Client,
  type Conversation,
} from "@xmtp/browser-sdk";

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

// #region group-messages
export async function streamGroupMessages(
  client: Client,
  group: Group,
  handleMessage: (message: Message) => Promise<void>,
  signal: AbortSignal,
) {
  const stream = MessageStream.openGroup(client, group, undefined, { signal });
  try {
    await stream.onValue(async (message) => {
      await handleMessage(message);
    });
  } finally {
    await stream.end();
  }
}
// #endregion group-messages

// #region all-messages
export async function streamAllMessages(
  client: Client,
  handleMessage: (message: Message) => Promise<void>,
  signal: AbortSignal,
) {
  const stream = MessageStream.open(
    client,
    { consentStates: ["allowed"] },
    { signal },
  );
  try {
    await stream.onValue(async (message) => {
      await handleMessage(message);
    });
  } finally {
    await stream.end();
  }
}
// #endregion all-messages
