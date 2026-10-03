import {
  ConversationStream,
  MessageStream,
  type Client,
  type ConnectionState,
} from "@xmtp/node-sdk";

export async function checkStreamOptions(client: Client): Promise<void> {
  // @ts-expect-error Conversation streams have no pre-sync switch.
  ConversationStream.open(client, undefined, { disableSync: true });
  await client.events({
    kinds: ["consent.changed"],
    referencesOwnMessages: false,
    // @ts-expect-error Event filters have no pre-sync switch.
    disableSync: true,
  });
  // Core owns the retry policy of every message reader.
  // @ts-expect-error No host retry count.
  MessageStream.open(client, undefined, { retryAttempts: 1 });
  // @ts-expect-error No host retry delay.
  MessageStream.open(client, undefined, { retryDelay: 1 });
  // @ts-expect-error No host retry switch.
  MessageStream.open(client, undefined, { retryOnFail: false });
  // @ts-expect-error No host retry callback.
  MessageStream.open(client, undefined, { onRetry: () => {} });
  // @ts-expect-error No host sync switch.
  MessageStream.open(client, undefined, { disableSync: true });
  // @ts-expect-error Conversation recovery also stays in Core.
  ConversationStream.open(client, undefined, { retryAttempts: 1 });

  const stream = ConversationStream.open(client, undefined, {
    signal: new AbortController().signal,
    onClose: (reason) => {
      const _kind: "closed" | "failed" = reason.kind;
    },
    onConnectionStateChange: (_previous, current) => {
      const _state: ConnectionState = current;
    },
  });
  await stream.end();
}
