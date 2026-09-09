import type {
  ContentCodec,
  EncodedContent,
} from "@xmtp/content-type-primitives";
import {
  encodeMarkdown,
  encodeText,
  isMarkdown,
  isReaction,
  isReadReceipt,
  isRemoteAttachment,
  isReply,
  isText,
  isTransactionReference,
  isWalletSendCalls,
  ReactionAction,
  ReactionSchema,
  type Reaction,
  type ReadReceipt,
  type RemoteAttachment,
  type Reply,
  type TransactionReference,
  type WalletSendCalls,
} from "@xmtp/node-sdk";
import { filter, type DecodedMessageWithContent } from "@/core/filter";
import type { AgentBaseContext } from "./Agent";
import { ConversationContext } from "./ConversationContext";

/** Constructor values for a message context. */
export type MessageContextParams<
  MessageContentType = unknown,
  ContentTypes = unknown,
> = Omit<AgentBaseContext<ContentTypes>, "message"> & {
  /** The decoded message that emitted the event. */
  message: DecodedMessageWithContent<MessageContentType>;
};

/** Context for a decoded message delivered to agent middleware. */
export class MessageContext<
  MessageContentType = unknown,
  ContentTypes = unknown,
> extends ConversationContext<ContentTypes> {
  #message: DecodedMessageWithContent<MessageContentType>;

  /** Create a context from a decoded message and its conversation. */
  constructor({
    message,
    conversation,
    client,
  }: MessageContextParams<MessageContentType, ContentTypes>) {
    super({ conversation, client });
    this.#message = message;
  }

  /** Narrow the message when its encoded type id matches the supplied codec. */
  usesCodec<T extends ContentCodec>(
    codecClass: new () => T,
  ): this is MessageContext<ReturnType<T["decode"]>> {
    return filter.usesCodec(this.#message, codecClass);
  }

  /** Narrow the message to Markdown content. */
  isMarkdown(): this is MessageContext<string> {
    return isMarkdown(this.#message);
  }

  /** Narrow the message to plain text content. */
  isText(): this is MessageContext<string> {
    return isText(this.#message);
  }

  /** Narrow the message to a reply. */
  isReply(): this is MessageContext<Reply> {
    return isReply(this.#message);
  }

  /** Narrow the message to a reaction. */
  isReaction(): this is MessageContext<Reaction> {
    return isReaction(this.#message);
  }

  /** Narrow the message to a read receipt. */
  isReadReceipt(): this is MessageContext<ReadReceipt> {
    return isReadReceipt(this.#message);
  }

  /** Narrow the message to a remote attachment. */
  isRemoteAttachment(): this is MessageContext<RemoteAttachment> {
    return isRemoteAttachment(this.#message);
  }

  /** Narrow the message to a transaction reference. */
  isTransactionReference(): this is MessageContext<TransactionReference> {
    return isTransactionReference(this.#message);
  }

  /** Narrow the message to wallet send calls. */
  isWalletSendCalls(): this is MessageContext<WalletSendCalls> {
    return isWalletSendCalls(this.#message);
  }

  /** Send an `added` reaction that references this message. */
  async sendReaction(
    content: string,
    schema: Reaction["schema"] = ReactionSchema.Unicode,
  ) {
    const reaction: Reaction = {
      action: ReactionAction.Added,
      reference: this.#message.id,
      referenceInboxId: this.#message.senderInboxId,
      schema,
      content,
    };
    await this.conversation.sendReaction(reaction);
  }

  async #sendReply(content: EncodedContent) {
    await this.conversation.sendReply({
      content,
      reference: this.#message.id,
      referenceInboxId: this.#message.senderInboxId,
    });
  }

  /** Reply to this message with Markdown content. */
  async sendMarkdownReply(markdown: string) {
    await this.#sendReply(encodeMarkdown(markdown));
  }

  /** Reply to this message with plain text content. */
  async sendTextReply(text: string) {
    await this.#sendReply(encodeText(text));
  }

  /** Resolve the sender's first identifier from the local inbox state. */
  async getSenderAddress() {
    const inboxState = await this.client.preferences.getInboxStates([
      this.#message.senderInboxId,
    ]);
    return inboxState[0]?.identifiers[0]?.identifier;
  }

  /** Return the decoded message. */
  get message() {
    return this.#message;
  }
}
