import type {
  Agent,
  EventHandlerMap,
  ConsentChanged,
  MessageReceived,
  MessageStatusChanged,
} from "@xmtp/agent-sdk";

export type EventHandlers = {
  messageReceived: (change: MessageReceived) => Promise<void>;
  messageStatusChanged: (change: MessageStatusChanged) => Promise<void>;
  consentChanged: (change: ConsentChanged) => Promise<void>;
  refreshConversation: (groupId: Uint8Array) => Promise<void>;
  refreshAll: (discarded: bigint) => Promise<void>;
};

// #region listener
export async function listenToAgentClientEvents(
  agent: Agent,
  handlers: EventHandlers,
) {
  const listenerId = await agent.client.startListener(
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
  return () => agent.client.stopListener(listenerId);
}
// #endregion listener

// #region agent
export function listenToAgentMessages(
  agent: Agent,
  onText: (...args: EventHandlerMap<unknown>["text"]) => Promise<void>,
  onError: (error: Error) => void,
) {
  agent.on("text", onText);
  agent.on("unhandledError", onError);
  return () => {
    agent.off("text", onText);
    agent.off("unhandledError", onError);
  };
}
// #endregion agent
