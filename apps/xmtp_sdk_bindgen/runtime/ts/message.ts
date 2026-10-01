import {
  ErrorCategory,
  MessageBody_Tags,
  MessageContent_Tags,
  MessageContent,
  XmtpError,
  encodeText,
  type EncodedContent,
  type Conversation,
  type MessageBody,
  type MessageData,
  type MessageId,
  type ConversationId,
  type InboxId,
  type Reaction,
  type SendOptions,
} from "../xmtp_sdk";
import { ClientRegistry, type Client, type ContentCodec } from "./client";
import {
  liftCustomBody,
  liftCustomContent,
  type LiftedCustomBody,
  type LiftedCustomContent,
} from "./custom-lift";

type LiftedReplyBody =
  | Exclude<MessageBody, { tag: MessageBody_Tags.Custom }>
  | LiftedCustomBody;

function decodeReplyBody(
  body: MessageBody,
  clientKey: bigint,
): LiftedReplyBody {
  if (body.tag !== MessageBody_Tags.Custom) return body;
  const encoded = body.inner.encoded;
  const owner = ClientRegistry.get(clientKey);
  const decoded = owner?.decodeCustom(encoded);
  return liftCustomBody(body, owner !== undefined, decoded);
}

export class Message {
  readonly content:
    | Exclude<
        MessageContent,
        { tag: MessageContent_Tags.Custom | MessageContent_Tags.Reply }
      >
    | {
        tag: MessageContent_Tags.Reply;
        inner: { referenceId: MessageId; body: LiftedReplyBody };
      }
    | LiftedCustomContent;
  readonly inReplyToContent?: LiftedReplyBody;
  readonly replyContent?: LiftedReplyBody;

  constructor(readonly data: MessageData) {
    const parent = data.inReplyTo?.content;
    this.inReplyToContent =
      parent === undefined
        ? undefined
        : decodeReplyBody(parent, data.clientKey);
    const content = data.content;
    this.replyContent =
      content.tag === MessageContent_Tags.Reply
        ? decodeReplyBody(content.inner.body, data.clientKey)
        : undefined;
    if (
      this.replyContent?.tag === MessageBody_Tags.Custom &&
      this.replyContent.inner.error?.code === "CodecDecodeFailed"
    ) {
      this.content = MessageContent.Unknown.new({
        encoded: data.encoded,
        rawBytes: data.rawBytes,
        error: this.replyContent.inner.error,
      });
      return;
    }
    if (content.tag === MessageContent_Tags.Reply) {
      this.content = {
        tag: content.tag,
        inner: {
          referenceId: content.inner.referenceId,
          body: this.replyContent!,
        },
      };
      return;
    }
    if (content.tag !== MessageContent_Tags.Custom) {
      this.content = content;
      return;
    }
    const owner = ClientRegistry.get(data.clientKey);
    const decoded = owner?.decodeCustom(content.inner.encoded);
    this.content = liftCustomContent(content, owner !== undefined, decoded);
  }

  get deliveryCursor(): string | null {
    return this.data.deliveryCursor ?? null;
  }

  get id(): MessageId {
    return this.data.id;
  }

  get conversationId(): ConversationId {
    return this.data.conversationId;
  }

  get topic() {
    return this.data.topic;
  }

  get senderInboxId(): InboxId {
    return this.data.senderInboxId;
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

  get rawBytes() {
    return this.data.rawBytes;
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
    return this.client().conversations().getMessageById(this.id);
  }

  async delete(): Promise<MessageId> {
    return this.client().conversations().deleteMessage(this.id);
  }

  async deleteLocally(): Promise<void> {
    return this.client().conversations().deleteMessageLocally(this.id);
  }

  async react(reaction: Reaction, options?: SendOptions): Promise<MessageId> {
    return this.client()
      .conversations()
      .reactToMessage(this.id, reaction, options);
  }

  async reply(
    content: string | EncodedContent,
    options?: SendOptions,
  ): Promise<MessageId>;
  async reply<T>(
    codec: ContentCodec<T>,
    value: T,
    options?: SendOptions,
  ): Promise<MessageId>;
  async reply<T>(
    content: string | EncodedContent | ContentCodec<T>,
    valueOrOptions?: T | SendOptions,
    options?: SendOptions,
  ): Promise<MessageId> {
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
      : this.client().conversations().getMessageById(id);
  }

  async conversation(): Promise<Conversation | undefined> {
    return this.client().conversations().getById(this.conversationId);
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
