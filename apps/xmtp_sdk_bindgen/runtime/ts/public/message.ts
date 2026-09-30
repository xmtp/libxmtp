import {
  currentProjection,
  liftContentTypeId,
  liftDeliveryStatus,
  liftEncodedContent,
  liftMessageBody,
  liftMessageContent,
  liftMessageKind,
  liftReactionMessage,
  liftReplyParent,
  publicError,
  type ContentTypeId,
  type Conversation,
  type ConversationId,
  type DeliveryStatus,
  type EncodedContent,
  type InboxId,
  type MessageBody,
  type MessageContent,
  type MessageId,
  type MessageKind,
  type ObjectProjection,
  type Reaction,
  type ReactionMessage,
  type ReplyParent,
  type SendOptions,
} from "../../public-values.gen";
import {
  MessageBody_Tags,
  MessageContent_Tags,
  type MessageBody as BoundBody,
  type MessageContent as BoundContent,
} from "../../xmtp_sdk";
import type { LiftedCustomBody, LiftedCustomContent } from "../custom-lift";
import type { Timestamp } from "../ids";
import { publicClient, type Client } from "./client";
import type { ContentCodec } from "./codec";
import { encodeText, type BoundMessage } from "./host";

// A host reply can carry a decoded custom body (browser); only its tag is used.
type HostContent =
  | Exclude<
      BoundContent,
      { tag: MessageContent_Tags.Custom | MessageContent_Tags.Reply }
    >
  | { readonly tag: MessageContent_Tags.Reply }
  | LiftedCustomContent;
type HostBody =
  | Exclude<BoundBody, { tag: MessageBody_Tags.Custom }>
  | LiftedCustomBody;

function decoded(inner: { value?: unknown; error?: string }): {
  value?: unknown;
  error?: string;
} {
  return {
    ...("value" in inner ? { value: inner.value } : {}),
    ...(inner.error === undefined ? {} : { error: inner.error }),
  };
}

// The host decoded custom content with its client's codecs. Keep that value
// or error next to the public envelope.
function liftContent(
  bound: BoundMessage,
  projection: ObjectProjection,
): MessageContent {
  const content: HostContent = bound.content;
  // A public reply keeps the envelope body; `replyContent` has the decoded one.
  if (content.tag === MessageContent_Tags.Reply)
    return liftMessageContent(bound.data.content, projection);
  if (content.tag !== MessageContent_Tags.Custom)
    return liftMessageContent(content, projection);
  const { encoded, rawBytes } = content.inner;
  return {
    kind: "custom",
    encoded: liftEncodedContent(encoded, projection),
    rawBytes: new Uint8Array(rawBytes),
    ...decoded(content.inner),
  };
}

function liftBody(body: HostBody, projection: ObjectProjection): MessageBody {
  if (body.tag !== MessageBody_Tags.Custom)
    return liftMessageBody(body, projection);
  return {
    kind: "custom",
    encoded: liftEncodedContent(body.inner.encoded, projection),
    ...decoded(body.inner),
  };
}

function encodedText(text: string): EncodedContent {
  try {
    return liftEncodedContent(encodeText(text), currentProjection());
  } catch (error) {
    throw publicError(error);
  }
}

const bindings = new WeakMap<Message, BoundMessage>();
let create!: (bound: BoundMessage) => Message;

/**
 * A received or stored message. Its fields are public values. Its actions use
 * the client that returned it; the message holds that client weakly, so an
 * action after the client ends or is collected fails with `ClientClosed`.
 */
export class Message {
  readonly id: MessageId;
  readonly conversationId: ConversationId;
  readonly topic: string;
  readonly senderInboxId: InboxId;
  readonly sentAt: Timestamp;
  readonly insertedAt: Timestamp;
  readonly expiresAt?: Timestamp;
  readonly kind: MessageKind;
  readonly deliveryStatus: DeliveryStatus;
  readonly contentType: ContentTypeId;
  readonly fallback?: string;
  readonly encoded: EncodedContent;
  readonly content: MessageContent;
  readonly replyCount: bigint;
  readonly reactions: ReactionMessage[];
  readonly inReplyTo?: ReplyParent;
  /** The decoded body of the message this one replies to. */
  readonly inReplyToContent?: MessageBody;
  /** The decoded body of this reply. */
  readonly replyContent?: MessageBody;
  /** The committed delivery position, or null when there is none. */
  readonly deliveryCursor: string | null;

  static {
    create = (bound) => new Message(bound);
  }

  private constructor(bound: BoundMessage) {
    bindings.set(this, bound);
    const projection = currentProjection();
    const data = bound.data;
    this.id = data.id;
    this.conversationId = data.conversationId;
    this.topic = data.topic;
    this.senderInboxId = data.senderInboxId;
    this.sentAt = data.sentAt;
    this.insertedAt = data.insertedAt;
    if (data.expiresAt !== undefined) this.expiresAt = data.expiresAt;
    this.kind = liftMessageKind(data.kind, projection);
    this.deliveryStatus = liftDeliveryStatus(data.deliveryStatus, projection);
    this.contentType = liftContentTypeId(data.contentType, projection);
    if (data.fallback !== undefined) this.fallback = data.fallback;
    this.encoded = liftEncodedContent(data.encoded, projection);
    this.content = liftContent(bound, projection);
    this.replyCount = data.replyCount;
    this.reactions = data.reactions.map((reaction) =>
      liftReactionMessage(reaction, projection),
    );
    if (data.inReplyTo !== undefined)
      this.inReplyTo = liftReplyParent(data.inReplyTo, projection);
    if (bound.inReplyToContent !== undefined)
      this.inReplyToContent = liftBody(bound.inReplyToContent, projection);
    if (bound.replyContent !== undefined)
      this.replyContent = liftBody(bound.replyContent, projection);
    this.deliveryCursor = data.deliveryCursor ?? null;
  }

  async refresh(): Promise<Message | undefined> {
    return this.client().conversations.getMessageById(this.id);
  }

  async delete(): Promise<MessageId> {
    return this.client().conversations.deleteMessage(this.id);
  }

  async deleteLocally(): Promise<void> {
    return this.client().conversations.deleteMessageLocally(this.id);
  }

  async react(reaction: Reaction, options?: SendOptions): Promise<MessageId> {
    return this.client().conversations.reactToMessage(
      this.id,
      reaction,
      options,
    );
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
        ? encodedText(content)
        : isCodec
          ? content.encode(valueOrOptions as T)
          : content;
    const sendOptions = isCodec
      ? options
      : (valueOrOptions as SendOptions | undefined);
    return this.client().conversations.replyToMessage(
      this.id,
      encoded,
      sendOptions,
    );
  }

  async parent(): Promise<Message | undefined> {
    const id = this.inReplyTo?.id;
    return id === undefined
      ? undefined
      : this.client().conversations.getMessageById(id);
  }

  async conversation(): Promise<Conversation | undefined> {
    return this.client().conversations.getById(this.conversationId);
  }

  /** The client that returned this message. Throws `ClientClosed` when gone. */
  client(): Client {
    try {
      return publicClient(boundMessage(this).client());
    } catch (error) {
      throw publicError(error);
    }
  }
}

export function liftBoundMessage(bound: BoundMessage): Message {
  return create(bound);
}

export function boundMessage(message: Message): BoundMessage {
  const bound = bindings.get(message);
  if (bound === undefined) throw new TypeError("not an XMTP Message");
  return bound;
}
