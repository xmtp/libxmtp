import {
  ConversationStream,
  MessageStream,
  type Client,
  type Conversation,
  type Group,
  type Dm,
  type DeliveryCursor,
  type ConnectionState,
  type EventFilter,
} from "@xmtp/browser-sdk";

export async function checkStreamOptions(client: Client): Promise<void> {
  // @ts-expect-error Conversation streams have no pre-sync switch.
  client.conversations.stream({ disableSync: true });
  await client.events({
    kinds: ["consent.changed"],
    references_own_messages: false,
    // @ts-expect-error Event filters have no pre-sync switch.
    disableSync: true,
  });
  // Core owns the retry policy of every message reader.
  // @ts-expect-error No host retry count.
  client.conversations.streamAllMessages({ retryAttempts: 1 });
  // @ts-expect-error No host retry delay.
  client.conversations.streamAllMessages({ retryDelay: 1 });
  // @ts-expect-error No host retry switch.
  client.conversations.streamAllMessages({ retryOnFail: false });
  // @ts-expect-error No host retry callback.
  client.conversations.streamAllMessages({ onRetry: () => {} });
  // @ts-expect-error No host sync switch.
  client.conversations.streamAllMessages({ disableSync: true });
  // @ts-expect-error Conversation recovery also stays in Core.
  client.conversations.stream({ retryAttempts: 1 });

  const stream = client.conversations.stream({
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

// EventFilter permits omission of references_own_messages.
export async function checkEventFilterDefault(client: Client) {
  const filter: EventFilter = {
    kinds: ["conversation.joined"],
  };
  const events = await client.events(filter);
  await events.return();
  await client.startListener(filter, () => {});
}

export function checkReceiverMethods(
  client: Client,
  group: Group,
  dm: Dm,
  conversation: Conversation,
  from: DeliveryCursor,
): void {
  const conversations: ConversationStream = client.conversations.stream();
  const messages: MessageStream = client.conversations.streamAllMessages();
  const scoped: MessageStream[] = [
    group.streamMessages(),
    dm.streamMessages(),
    conversation.streamMessages(),
  ];
  void conversations;
  void messages;
  void scoped;
  client.conversations.stream({ conversationKind: "dm", consentStates: [] });
  client.conversations.streamAllMessages({
    conversationKind: "group",
    consentStates: ["allowed"],
    from,
  });
  conversation.streamMessages({
    from,
    onClose: () => {},
    onConnectionStateChange: () => {},
  });
  // @ts-expect-error Conversation notifications have no replay cursor.
  client.conversations.stream({ from });
  // @ts-expect-error Scoped delivery has no kind filter.
  conversation.streamMessages({ conversationKind: "dm" });
  // @ts-expect-error Scoped delivery has no consent filter.
  group.streamMessages({ consentStates: [] });
  // @ts-expect-error The receiver determines its owner.
  void group.sdkStreamOwnerKey;
  // @ts-expect-error The receiver determines its owner.
  void dm.sdkStreamOwnerKey;
  // @ts-expect-error The receiver determines its owner.
  void client.conversations.sdkStreamOwnerKey;
  // @ts-expect-error Public static factories were removed.
  void ConversationStream.open;
  // @ts-expect-error Public static factories were removed.
  void MessageStream.open;
  // @ts-expect-error Public static factories were removed.
  void MessageStream.openGroup;
  // @ts-expect-error Public static factories were removed.
  void MessageStream.openDm;
}
