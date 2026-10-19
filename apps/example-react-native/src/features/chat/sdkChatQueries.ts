// The chat stores' SDK reads and writes.

import {
  type Client,
  type Conversation,
  Group,
  type ListMessagesOptions,
  type Message,
  type MessageBody,
  Timestamp,
} from "@xmtp/react-native-sdk";

import type {
  ChatPage,
  ChatQueries,
  OutboundContent,
  PendingPage,
} from "@/core/chatHistory.ts";
import {
  type ChatContent,
  ChatContentCopy,
  type ChatMessage,
  type ConversationRecord,
  type HistoryPosition,
  type MessageDelivery,
  type RecoveryPosition,
} from "@/core/chatMessages.ts";
import type { IdentityDirectory } from "@/core/identityDirectory.ts";
import { SendError } from "@/core/sendJournal.ts";

/** The text of a reply body or quoted parent. */
function bodyText(body: MessageBody, fallback: string | undefined): string {
  switch (body.kind) {
    case "text":
    case "markdown":
      return body.value;
    case "deletedMessage":
      return ChatContentCopy.deleted;
    default:
      return fallback ?? ChatContentCopy.unsupportedReply;
  }
}

/** What a message row shows. */
export function chatContent(message: Message): ChatContent {
  const content = message.content;
  switch (content.kind) {
    case "text":
      return { kind: "text", text: content.value };
    case "markdown":
      return { kind: "markdown", text: content.value };
    case "reply": {
      const parent = message.inReplyTo;
      return {
        kind: "reply",
        quote:
          parent === undefined
            ? ChatContentCopy.replyFallback
            : bodyText(parent.content, parent.fallback),
        body: bodyText(content.body, message.fallback),
        parentId: content.referenceId,
      };
    }
    case "remoteAttachment":
      return { kind: "attachment", file: content.value };
    case "deletedMessage":
      return { kind: "deleted" };
    case "reaction":
      return { kind: "reaction" };
    default:
      return {
        kind: "unsupported",
        text: message.fallback ?? ChatContentCopy.unsupportedContent,
      };
  }
}

function delivery(message: Message): MessageDelivery {
  switch (message.deliveryStatus) {
    case "published":
      return "published";
    case "failed":
      return "failed";
    default:
      return "queued";
  }
}

/** The chat row of an SDK message. */
export function chatMessage(message: Message): ChatMessage {
  const cursor = message.deliveryCursor;
  return {
    id: message.id,
    conversationId: message.conversationId,
    sender: message.senderInboxId,
    sentAtNs: message.sentAt.ns,
    insertedAtNs: message.insertedAt.ns,
    delivery: delivery(message),
    content: chatContent(message),
    ...(cursor === null
      ? {}
      : { position: { sentAtNs: message.sentAt.ns, cursor } }),
    reactions: message.reactions.map((reaction) => ({
      sender: reaction.senderInboxId,
      emoji: reaction.reaction.content,
      schema: reaction.reaction.schema,
      added: reaction.reaction.action === "added",
      sentAtNs: reaction.sentAt.ns,
    })),
  };
}

function historyOptions(ascending: boolean): ListMessagesOptions {
  return {
    limit: 50,
    direction: ascending ? "ascending" : "descending",
    sortBy: "sentAt",
    deliveryStatus: "published",
    kind: "application",
  };
}

const pendingOptions: ListMessagesOptions = {
  limit: 50,
  direction: "descending",
  sortBy: "sentAt",
  kind: "application",
};

function sdkPosition(position: HistoryPosition | undefined) {
  return position === undefined
    ? undefined
    : {
        sentAt: new Timestamp(position.sentAtNs),
        deliveryCursor: position.cursor,
      };
}

/**
 * Debug-only failure injection for checks that need a failed send (the
 * shared backend cannot fail on demand). `dev/chat-probe` sets it.
 */
export const chatDebug = {
  /** The next queue call throws an SDK-like error: the outcome is unknown. */
  failNextQueue: false,
  /** Publishing fails (the message stays unpublished) until this is false. */
  failPublish: false,
};

export class SdkChatQueries implements ChatQueries {
  readonly #client: Client;
  readonly #identities: IdentityDirectory;

  constructor(client: Client, identities: IdentityDirectory) {
    this.#client = client;
    this.#identities = identities;
  }

  async #get(id: string): Promise<Conversation> {
    const value = await this.#client.conversations.getById(id);
    if (value === undefined) throw new SendError("unavailable");
    return value;
  }

  async conversation(id: string): Promise<ConversationRecord> {
    const value = await this.#get(id);
    if (value instanceof Group) {
      const state = await value.state();
      let memberCount: number | undefined;
      try {
        memberCount = (await value.members()).length;
      } catch {
        memberCount = undefined;
      }
      return {
        id,
        kind: "group",
        name: state.name,
        consent: state.common.consentState,
        ...(memberCount === undefined ? {} : { memberCount }),
      };
    }
    const state = await value.state();
    const peer = await value.peerInboxId();
    let peerHandle: string | undefined;
    if (peer !== null) {
      try {
        await this.#identities.load([peer]);
        peerHandle = this.#identities.identity(peer)?.handle;
      } catch {
        peerHandle = undefined;
      }
    }
    return {
      id,
      kind: "dm",
      consent: state.consentState,
      ...(peer === null ? {} : { peerInboxId: peer }),
      ...(peerHandle === undefined ? {} : { peerHandle }),
    };
  }

  async history(
    id: string,
    ascending: boolean,
    before?: HistoryPosition,
    after?: HistoryPosition,
  ): Promise<ChatPage> {
    const value = await this.#get(id);
    const page = await value.messageHistoryPage(
      historyOptions(ascending),
      sdkPosition(before),
      sdkPosition(after),
    );
    const first = page.firstPosition;
    const last = page.lastPosition;
    return {
      rows: page.messages.map(chatMessage),
      ...(first === undefined
        ? {}
        : {
            first: { sentAtNs: first.sentAt.ns, cursor: first.deliveryCursor },
          }),
      ...(last === undefined
        ? {}
        : { last: { sentAtNs: last.sentAt.ns, cursor: last.deliveryCursor } }),
      hasMore: page.hasMore,
      skipped: page.skippedCount,
    };
  }

  async pending(id: string, before?: RecoveryPosition): Promise<PendingPage> {
    const value = await this.#get(id);
    const page = await value.messageRecoveryPage(
      pendingOptions,
      before === undefined
        ? undefined
        : {
            sentAt: new Timestamp(before.sentAtNs),
            messageCursor: before.cursor,
          },
      undefined,
    );
    const last = page.lastPosition;
    return {
      rows: page.messages.map(chatMessage),
      ...(last === undefined
        ? {}
        : {
            last: { sentAtNs: last.sentAt.ns, cursor: last.messageCursor },
          }),
      hasMore: page.hasMore,
      skipped: page.skippedCount,
    };
  }

  async message(id: string): Promise<ChatMessage | undefined> {
    const value = await this.#client.conversations.getMessageById(id);
    return value === undefined ? undefined : chatMessage(value);
  }

  async #parent(conversationId: string, parentId: string): Promise<Message> {
    const parent = await this.#client.conversations.getMessageById(parentId);
    if (parent?.conversationId !== conversationId) {
      throw new SendError("unavailable");
    }
    return parent;
  }

  async queue(
    conversationId: string,
    content: OutboundContent,
  ): Promise<string> {
    const value = await this.#get(conversationId);
    if (__DEV__ && chatDebug.failNextQueue) {
      chatDebug.failNextQueue = false;
      throw new Error("Debug: the send failed after it may have queued.");
    }
    const options = { optimistic: true };
    switch (content.kind) {
      case "text":
        return value.sendText(content.text, options);
      case "reply":
        return (await this.#parent(conversationId, content.parentId)).reply(
          content.text,
          options,
        );
      case "reaction":
        return (await this.#parent(conversationId, content.parentId)).react(
          {
            content: content.emoji,
            action: content.remove ? "removed" : "added",
            schema: "unicode",
          },
          options,
        );
      case "attachment":
        return value.sendRemoteAttachment(content.file, options);
    }
  }

  async publish(conversationId: string, messageId: string): Promise<void> {
    if (__DEV__ && chatDebug.failPublish) {
      throw new Error("Debug: publishing is off.");
    }
    await (await this.#get(conversationId)).publishMessage(messageId);
  }

  /** Acceptance only; the journal publishes the returned deletion ID. */
  async delete(messageId: string): Promise<string> {
    const message = await this.#client.conversations.getMessageById(messageId);
    if (
      message?.senderInboxId !== this.#client.inboxId ||
      message.deliveryStatus !== "published"
    ) {
      throw new SendError("rejected");
    }
    return message.delete();
  }
}
