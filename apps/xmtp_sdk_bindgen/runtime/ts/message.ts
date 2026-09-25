import {
  ErrorCategory,
  MessageBody_Tags,
  MessageContent_Tags,
  XmtpError,
  encodeText,
  type EncodedContent,
  type MessageContent,
  type MessageBody,
  type MessageData,
  type Reaction,
  type SendOptions,
} from "../xmtp_sdk";
import { ClientRegistry, type Client, type ContentCodec } from "./client";
import type { MessageID } from "./ids";

export class Message {
  readonly content:
    | MessageContent
    | {
        tag: MessageContent_Tags.Custom;
        inner: { encoded: EncodedContent; value?: unknown; error?: string };
      };
  readonly inReplyToContent?:
    | MessageBody
    | {
        tag: MessageBody_Tags.Custom;
        inner: { encoded: EncodedContent; value?: unknown; error?: string };
      };

  constructor(readonly data: MessageData) {
    const parent = data.inReplyTo?.content;
    if (parent?.tag === MessageBody_Tags.Custom) {
      const encoded = parent.inner.encoded;
      const decoded = ClientRegistry.get(data.clientKey)?.decodeCustom(encoded);
      this.inReplyToContent = {
        tag: MessageBody_Tags.Custom,
        inner: { encoded, ...(decoded ?? { error: "clientClosed" }) },
      };
    } else {
      this.inReplyToContent = parent;
    }
    const content = data.content;
    if (content.tag !== MessageContent_Tags.Custom) {
      this.content = content;
      return;
    }
    const encoded = content.inner.encoded;
    const owner = ClientRegistry.get(data.clientKey);
    const decoded = owner?.decodeCustom(encoded);
    this.content =
      decoded === undefined && owner !== undefined
        ? content
        : {
            tag: MessageContent_Tags.Custom,
            inner: { encoded, ...(decoded ?? { error: "clientClosed" }) },
          };
  }

  get id() {
    return this.data.id;
  }

  get conversationID() {
    return this.data.conversationID;
  }

  get topic() {
    return this.data.topic;
  }

  get senderInboxID() {
    return this.data.senderInboxID;
  }

  get sentAt() {
    return this.data.sentAt;
  }

  get kind() {
    return this.data.kind;
  }

  get deliveryStatus() {
    return this.data.deliveryStatus;
  }

  get contentType() {
    return this.data.contentType;
  }

  get fallback() {
    return this.data.fallback;
  }

  get encoded() {
    return this.data.encoded;
  }

  get replyCount() {
    return this.data.replyCount;
  }

  get reactions() {
    return this.data.reactions;
  }

  get insertedAt() {
    return this.data.insertedAt;
  }

  get expiresAt() {
    return this.data.expiresAt;
  }

  get inReplyTo() {
    return this.data.inReplyTo;
  }

  async refresh(): Promise<Message | undefined> {
    return this.client().conversations().getMessageByID(this.id);
  }

  async delete(): Promise<MessageID> {
    return this.client().conversations().deleteMessage(this.id);
  }

  async deleteLocally(): Promise<void> {
    return this.client().conversations().deleteMessageLocally(this.id);
  }

  async react(reaction: Reaction, options?: SendOptions): Promise<MessageID> {
    return this.client()
      .conversations()
      .reactToMessage(this.id, reaction, options);
  }

  async reply(
    content: string | EncodedContent,
    options?: SendOptions,
  ): Promise<MessageID>;
  async reply<T>(
    codec: ContentCodec<T>,
    value: T,
    options?: SendOptions,
  ): Promise<MessageID>;
  async reply<T>(
    content: string | EncodedContent | ContentCodec<T>,
    valueOrOptions?: T | SendOptions,
    options?: SendOptions,
  ): Promise<MessageID> {
    const isCodec = typeof content !== "string" && "encode" in content;
    const encoded =
      typeof content === "string"
        ? encodeText(content)
        : isCodec
          ? content.encode(valueOrOptions as T)
          : content;
    const sendOptions = isCodec
      ? options
      : (valueOrOptions as SendOptions | undefined);
    return this.client()
      .conversations()
      .replyToMessage(this.id, encoded, sendOptions);
  }

  async parent(): Promise<Message | undefined> {
    const id = this.inReplyTo?.id;
    return id === undefined
      ? undefined
      : this.client().conversations().getMessageByID(id);
  }

  async conversation(): Promise<object | undefined> {
    return this.client().conversations().getByID(this.conversationID);
  }

  client(): Client {
    const client = ClientRegistry.get(this.data.clientKey);
    if (client === undefined)
      throw new XmtpError.ClientClosed({
        code: "ClientClosed",
        category: ErrorCategory.Lifecycle,
        retryable: false,
        message: "client is closed",
      });
    return client;
  }
}
