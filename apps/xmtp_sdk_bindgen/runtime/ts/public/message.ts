import {
  MessageFields,
  currentProjection,
  liftEncodedContent,
  liftErrorDetails,
  liftMessageBody,
  liftMessageContent,
  publicError,
  type Conversation,
  type EncodedContent,
  type MessageBody,
  type MessageContent,
  type MessageId,
  type ObjectProjection,
  type Reaction,
  type SendOptions,
} from "../../public-values.gen";
import {
  MessageBody_Tags,
  MessageContent_Tags,
  type MessageBody as BoundBody,
  type MessageContent as BoundContent,
} from "../../xmtp_sdk";
import type { ErrorDetails as BoundErrorDetails } from "../../xmtp_sdk";
import type { LiftedCustomBody, LiftedCustomContent } from "../custom-lift";
import { publicClient, type Client } from "./client";
import type { ContentCodec } from "./codec";
import { encodeForSend, isCodec as isCodecContent } from "./codec-policy";
import { encodeText, type BoundMessage } from "./host";

// A host reply can carry a decoded custom body (browser); only its tag is used.
type HostContent =
  | Exclude<
      BoundContent,
      { tag: MessageContent_Tags.Custom | MessageContent_Tags.Reply }
    >
  | {
      readonly tag: MessageContent_Tags.Reply;
      readonly inner: {
        readonly referenceId: MessageId;
        readonly body: HostBody;
      };
    }
  | LiftedCustomContent;
type HostBody =
  | Exclude<BoundBody, { tag: MessageBody_Tags.Custom }>
  | LiftedCustomBody;

function decoded(
  inner: { value?: unknown; error?: BoundErrorDetails },
  projection: ObjectProjection,
) {
  return {
    ...("value" in inner ? { value: inner.value } : {}),
    ...(inner.error === undefined
      ? {}
      : { error: liftErrorDetails(inner.error, projection) }),
  };
}

// The host decoded custom content with its client's codecs. Keep that value
// or error next to the public envelope.
function liftContent(
  bound: BoundMessage,
  projection: ObjectProjection,
): MessageContent {
  const content: HostContent = bound.content;
  if (content.tag === MessageContent_Tags.Reply)
    return {
      kind: "reply",
      referenceId: content.inner.referenceId,
      body: liftBody(content.inner.body, projection),
    };
  if (content.tag !== MessageContent_Tags.Custom)
    return liftMessageContent(content, projection);
  const { encoded, rawBytes } = content.inner;
  return {
    kind: "custom",
    encoded: liftEncodedContent(encoded, projection),
    rawBytes: new Uint8Array(rawBytes),
    ...decoded(content.inner, projection),
  };
}

function liftBody(body: HostBody, projection: ObjectProjection): MessageBody {
  if (body.tag !== MessageBody_Tags.Custom)
    return liftMessageBody(body, projection);
  return {
    kind: "custom",
    encoded: liftEncodedContent(body.inner.encoded, projection),
    rawBytes: new Uint8Array(body.inner.rawBytes),
    ...decoded(body.inner, projection),
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
 * The generated base holds the message's fields.
 */
export class Message extends MessageFields {
  readonly content: MessageContent;
  /** The decoded body of the message this one replies to. */
  readonly inReplyToContent?: MessageBody;
  /** The decoded body of this reply. */
  readonly replyContent?: MessageBody;

  static {
    create = (bound) => new Message(bound);
  }

  private constructor(bound: BoundMessage) {
    super(bound.data, currentProjection());
    bindings.set(this, bound);
    const projection = currentProjection();
    this.content = liftContent(bound, projection);
    if (bound.inReplyToContent !== undefined)
      this.inReplyToContent = liftBody(bound.inReplyToContent, projection);
    if (bound.replyContent !== undefined)
      this.replyContent = liftBody(bound.replyContent, projection);
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
  /**
   * Reply with a value of a typed codec. The codec's fallback applies to the
   * nested envelope; the reply keeps the reply type's push default unless
   * `options.shouldPush` is set. A failed codec step is `CodecEncodeFailed`,
   * with no publish attempt.
   */
  async reply<T>(
    codec: ContentCodec<T>,
    value: NoInfer<T>,
    options?: SendOptions,
  ): Promise<MessageId>;
  async reply<T>(
    content: string | EncodedContent | ContentCodec<T>,
    valueOrOptions?: T | SendOptions,
    options?: SendOptions,
  ): Promise<MessageId> {
    // The shared check turns a throwing check into CodecEncodeFailed.
    const isCodec = isCodecContent(content);
    const encoded =
      typeof content === "string"
        ? encodedText(content)
        : isCodec
          ? encodeForSend(content, valueOrOptions as T)
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
