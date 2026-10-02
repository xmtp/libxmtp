import {
  MarkdownCodec,
  TextCodec,
  type AnyContentCodec,
  type EncodedContent,
  type Message,
  type MessageContent,
  type Reaction,
  type RemoteAttachment,
  type TransactionReference,
  type WalletSendCalls,
} from "@xmtp/node-sdk";

import { filter } from "@/core/filter";

import type { AgentBaseContext } from "./Agent";
import { ConversationContext } from "./ConversationContext";

export type MessageContextParams<
  _Content = unknown,
  ContentTypes = unknown,
> = Omit<AgentBaseContext<ContentTypes>, "message"> & { message: Message };

/** A message and its client for agent middleware. */
export class MessageContext<
  Content = unknown,
  ContentTypes = unknown,
> extends ConversationContext<ContentTypes> {
  #message: Message;
  #contentOverride?: { value: Content };
  constructor({
    message,
    conversation,
    client,
  }: MessageContextParams<Content, ContentTypes>) {
    super({ conversation, client });
    this.#message = message;
  }
  usesCodec<T extends AnyContentCodec>(
    codecClass: new () => T,
  ): this is MessageContext<ReturnType<T["decode"]>, ContentTypes> {
    return filter.usesCodec(this.#message, codecClass);
  }
  isMarkdown(): this is MessageContext<string, ContentTypes> {
    return this.#message.content.kind === "markdown";
  }
  isText(): this is MessageContext<string, ContentTypes> {
    return this.#message.content.kind === "text";
  }
  isReply(): this is MessageContext<
    Extract<MessageContent, { kind: "reply" }>,
    ContentTypes
  > {
    return this.#message.content.kind === "reply";
  }
  isReaction(): this is MessageContext<Reaction, ContentTypes> {
    return this.#message.content.kind === "reaction";
  }
  isReadReceipt(): this is MessageContext<undefined, ContentTypes> {
    return this.#message.content.kind === "readReceipt";
  }
  isRemoteAttachment(): this is MessageContext<RemoteAttachment, ContentTypes> {
    return this.#message.content.kind === "remoteAttachment";
  }
  isTransactionReference(): this is MessageContext<
    TransactionReference,
    ContentTypes
  > {
    return this.#message.content.kind === "transactionReference";
  }
  isWalletSendCalls(): this is MessageContext<WalletSendCalls, ContentTypes> {
    return this.#message.content.kind === "walletSendCalls";
  }
  async sendReaction(content: string, schema: Reaction["schema"] = "unicode") {
    await this.conversation.sendReaction(
      this.#message.id,
      this.#message.senderInboxId,
      { action: "added", schema, content },
      { shouldPush: false },
    );
  }
  async #sendReply(content: EncodedContent) {
    await this.conversation.sendReply(
      this.#message.id,
      this.#message.senderInboxId,
      content,
      { shouldPush: false },
    );
  }
  async sendMarkdownReply(markdown: string) {
    await this.#sendReply(new MarkdownCodec().encode(markdown));
  }
  async sendTextReply(text: string) {
    await this.#sendReply(new TextCodec().encode(text));
  }
  async getSenderAddress() {
    const states = await this.client.inboxStates(
      [this.#message.senderInboxId],
      false,
    );
    return states[0]?.identities[0]?.identifier;
  }
  /** Replace the middleware value without changing the SDK message. */
  set content(value: Content) {
    this.#contentOverride = { value };
  }

  /** The SDK message, including its tagged content and retained bytes. */
  get message() {
    return this.#message;
  }
  /** The decoded value selected by the content type. */
  get content(): Content {
    if (this.#contentOverride) return this.#contentOverride.value;
    const value = this.#message.content;
    if (value.kind === "reaction") return value.reaction as Content;
    if ("value" in value) return value.value as Content;
    return (value.kind === "reply" ? value : undefined) as Content;
  }
}
