import type {
  Client,
  ConsentChanged,
  MessageReceived,
  MessageStatusChanged,
} from "@xmtp/node-sdk";

export type EventHandlers = {
  messageReceived: (change: MessageReceived) => Promise<void>;
  messageStatusChanged: (change: MessageStatusChanged) => Promise<void>;
  consentChanged: (change: ConsentChanged) => Promise<void>;
  refreshConversation: (groupId: Uint8Array) => Promise<void>;
  refreshAll: (discarded: bigint) => Promise<void>;
};

// #region listener
export async function listenToClientEvents(
  client: Client,
  handlers: EventHandlers,
) {
  const listenerId = await client.startListener(
    {
      kinds: [
        "message.received",
        "message.status_changed",
        "consent.changed",
        "conversation.joined",
        "conversation.removed",
        "conversation.membership_changed",
        "conversation.metadata_changed",
      ],
    },
    async (event) => {
      switch (event.kind) {
        case "message.received":
          await handlers.messageReceived(event.message_received);
          break;
        case "message.status_changed":
          await handlers.messageStatusChanged(event.message_status_changed);
          break;
        case "consent.changed":
          await handlers.consentChanged(event.consent_changed);
          break;
        case "conversation.joined":
          await handlers.refreshConversation(
            event.conversation_joined.group_id,
          );
          break;
        case "conversation.removed":
          await handlers.refreshConversation(
            event.conversation_removed.group_id,
          );
          break;
        case "conversation.membership_changed":
          await handlers.refreshConversation(event.membership_changed.group_id);
          break;
        case "conversation.metadata_changed":
          await handlers.refreshConversation(event.metadata_changed.group_id);
          break;
        case "lagged":
          await handlers.refreshAll(event.lagged.discarded);
          break;
        default:
          break;
      }
    },
  );
  return () => client.stopListener(listenerId);
}
// #endregion listener

// #region refresh
export async function readConversationState(
  client: Client,
  groupId: Uint8Array,
) {
  const id = Array.from(groupId, (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  const conversation = await client.conversations.getById(id);
  return conversation ? await conversation.state() : undefined;
}
// #endregion refresh

// #region iterator
export async function readEvents(
  client: Client,
  handleStatus: (change: MessageStatusChanged) => Promise<void>,
  refreshAll: (discarded: bigint) => Promise<void>,
) {
  const stream = await client.events({ kinds: ["message.status_changed"] });
  try {
    for await (const event of stream) {
      if (event.kind === "message.status_changed") {
        await handleStatus(event.message_status_changed);
      } else if (event.kind === "lagged") {
        await refreshAll(event.lagged.discarded);
      }
    }
  } finally {
    await stream.return();
  }
}
// #endregion iterator
