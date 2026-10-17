import {
  type Message,
  type Group,
  type Client,
  type Conversation,
} from "@xmtp/node-sdk";

export async function streamConversations(
  client: Client,
  handleConversation: (conversation: Conversation) => void,
) {
  // #region stream
  const stream = client.conversations.stream();
  const receive = (async () => {
    for await (const conversation of stream) handleConversation(conversation);
  })();
  // #endregion stream
  return { stream, receive };
}

// #region group-messages
export async function streamGroupMessages(
  group: Group,
  handleMessage: (message: Message) => Promise<void>,
  signal: AbortSignal,
) {
  const stream = group.streamMessages({ signal });
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
  const stream = client.conversations.streamAllMessages({
    consentStates: ["allowed"],
    signal,
  });
  try {
    await stream.onValue(async (message) => {
      await handleMessage(message);
    });
  } finally {
    await stream.end();
  }
}
// #endregion all-messages
