import * as Pure from "../typescript-pure/xmtp_sdk.js";
import type { Client } from "./proxy.gen.js";
import type { MainSession } from "./runtime/bridge/main/session.js";
import {
  codecKey,
  decodeCustom,
  type AnyCodec,
} from "./runtime/custom-codec.js";
import {
  liftCustomBody,
  liftCustomContent,
  type LiftedCustomBody,
  type LiftedCustomContent,
} from "./runtime/custom-lift.js";
import * as B from "./xmtp_sdk.js";

export interface ContentCodec<T = unknown> {
  readonly type: B.ContentTypeID;
  encode(value: T): B.EncodedContent;
  decode(encoded: B.EncodedContent): T;
}

export type HostClientOptions = B.ClientOptions & {
  codecs?: readonly AnyCodec[];
};

declare const process: { cwd(): string } | undefined;

export function resolveBrowserOptions(
  options: B.ClientOptions,
): B.ClientOptions {
  if (options.storage.location.tag !== B.StorageLocation_Tags.Default)
    return options;
  const directory =
    typeof process === "undefined" ? "xmtp-sdk" : `${process.cwd()}/xmtp`;
  return {
    ...options,
    storage: {
      ...options.storage,
      location: B.StorageLocation.Directory.new(directory),
    },
  };
}

interface Owner {
  client: WeakRef<Client>;
  codecs: ReadonlyMap<string, AnyCodec>;
}

const owners = new WeakMap<MainSession, Map<bigint, Owner>>();

export function registerClient(
  session: MainSession,
  client: Client,
  codecs: readonly AnyCodec[],
): void {
  const entries = owners.get(session) ?? new Map<bigint, Owner>();
  entries.set(client.clientKey(), {
    client: new WeakRef(client),
    codecs: new Map(codecs.map((codec) => [codecKey(codec.type), codec])),
  });
  owners.set(session, entries);
}

export function unregisterClient(session: MainSession, key: bigint): void {
  owners.get(session)?.delete(key);
}

function closed(): B.XmtpError {
  return B.XmtpError.ClientClosed.new({
    code: "ClientClosed",
    category: B.ErrorCategory.Lifecycle,
    retryable: false,
    message: "client is closed",
  });
}

function owner(session: MainSession, key: bigint): Owner | undefined {
  const entry = owners.get(session)?.get(key);
  if (entry && !entry.client.deref()) {
    owners.get(session)?.delete(key);
    return undefined;
  }
  return entry;
}

type LiftedReplyBody =
  | Exclude<B.MessageBody, { tag: B.MessageBody_Tags.Custom }>
  | LiftedCustomBody;
type HostReply = {
  tag: B.MessageContent_Tags.Reply;
  inner: { referenceID: B.MessageID; body: LiftedReplyBody };
};
type HostContent =
  | Exclude<
      B.MessageContent,
      {
        tag: B.MessageContent_Tags.Custom | B.MessageContent_Tags.Reply;
      }
    >
  | HostReply
  | LiftedCustomContent;

function decodeContent(
  session: MainSession,
  key: bigint,
  content: B.MessageContent,
  encoded: B.EncodedContent,
): HostContent {
  if (content.tag === B.MessageContent_Tags.Custom) {
    const entry = owner(session, key);
    return liftCustomContent(
      content,
      entry !== undefined,
      decodeCustom(entry?.codecs, content.inner.encoded),
    );
  }
  // Deleted messages keep their original encoded bytes. Keep the Rust marker.
  if (
    content.tag === B.MessageContent_Tags.Unknown ||
    content.tag === B.MessageContent_Tags.DeletedMessage
  )
    return content;

  // Standard bytes are decoded by the main-thread pure WASM module.
  const standard = Pure.decodeStandard(encoded);
  switch (standard.tag) {
    case Pure.StandardContent_Tags.Text:
      return B.MessageContent.Text.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Markdown:
      return B.MessageContent.Markdown.new(standard.inner[0]);
    case Pure.StandardContent_Tags.ReadReceipt:
      return B.MessageContent.ReadReceipt.new();
    case Pure.StandardContent_Tags.Reaction:
      if (content.tag !== B.MessageContent_Tags.Reaction) return content;
      return B.MessageContent.Reaction.new({
        reference: content.inner.reference,
        referenceInboxID: content.inner.referenceInboxID,
        reaction: standard.inner.reaction,
      });
    case Pure.StandardContent_Tags.Attachment:
      return B.MessageContent.Attachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.RemoteAttachment:
      return B.MessageContent.RemoteAttachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.MultiRemoteAttachment:
      return B.MessageContent.MultiRemoteAttachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.TransactionReference:
      return B.MessageContent.TransactionReference.new(standard.inner[0]);
    case Pure.StandardContent_Tags.WalletSendCalls:
      return B.MessageContent.WalletSendCalls.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Actions:
      return B.MessageContent.Actions.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Intent:
      return B.MessageContent.Intent.new(standard.inner[0]);
    case Pure.StandardContent_Tags.GroupUpdated:
      return B.MessageContent.GroupUpdated.new(standard.inner[0]);
    case Pure.StandardContent_Tags.LeaveRequest:
      return B.MessageContent.LeaveRequest.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Reply:
      if (content.tag !== B.MessageContent_Tags.Reply) return content;
      return {
        tag: B.MessageContent_Tags.Reply,
        inner: {
          referenceID: content.inner.referenceID,
          body: decodeBody(
            session,
            key,
            content.inner.body,
            standard.inner.content,
          ),
        },
      };
    case Pure.StandardContent_Tags.DeleteMessage:
      return content;
  }
}

function decodeBody(
  session: MainSession,
  key: bigint,
  body: B.MessageBody,
  encoded: B.EncodedContent,
): LiftedReplyBody {
  if (body.tag === B.MessageBody_Tags.Custom) {
    const entry = owner(session, key);
    return liftCustomBody(
      body,
      entry !== undefined,
      decodeCustom(entry?.codecs, body.inner.encoded),
    );
  }
  // A deleted reply parent also keeps its original encoded bytes.
  if (
    body.tag === B.MessageBody_Tags.Unknown ||
    body.tag === B.MessageBody_Tags.DeletedMessage
  )
    return body;
  const standard = Pure.decodeStandard(encoded);
  switch (standard.tag) {
    case Pure.StandardContent_Tags.Text:
      return B.MessageBody.Text.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Markdown:
      return B.MessageBody.Markdown.new(standard.inner[0]);
    case Pure.StandardContent_Tags.ReadReceipt:
      return B.MessageBody.ReadReceipt.new();
    case Pure.StandardContent_Tags.Attachment:
      return B.MessageBody.Attachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.RemoteAttachment:
      return B.MessageBody.RemoteAttachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.MultiRemoteAttachment:
      return B.MessageBody.MultiRemoteAttachment.new(standard.inner[0]);
    case Pure.StandardContent_Tags.TransactionReference:
      return B.MessageBody.TransactionReference.new(standard.inner[0]);
    case Pure.StandardContent_Tags.WalletSendCalls:
      return B.MessageBody.WalletSendCalls.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Actions:
      return B.MessageBody.Actions.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Intent:
      return B.MessageBody.Intent.new(standard.inner[0]);
    case Pure.StandardContent_Tags.GroupUpdated:
      return B.MessageBody.GroupUpdated.new(standard.inner[0]);
    case Pure.StandardContent_Tags.LeaveRequest:
      return B.MessageBody.LeaveRequest.new(standard.inner[0]);
    case Pure.StandardContent_Tags.Reaction:
    case Pure.StandardContent_Tags.Reply:
    case Pure.StandardContent_Tags.DeleteMessage:
      return body;
  }
}

export class Message extends B.Message {
  readonly content: HostContent;
  readonly inReplyToContent?: LiftedReplyBody;
  readonly replyContent?: LiftedReplyBody;

  constructor(
    data: B.MessageData,
    private readonly session: MainSession,
  ) {
    super(data);
    this.content = decodeContent(
      session,
      data.clientKey,
      data.content,
      data.encoded,
    );
    this.inReplyToContent = data.inReplyTo
      ? decodeBody(
          session,
          data.clientKey,
          data.inReplyTo.content,
          data.inReplyTo.encoded,
        )
      : undefined;
    this.replyContent =
      this.content.tag === B.MessageContent_Tags.Reply
        ? this.content.inner.body
        : undefined;
  }

  get conversationID(): B.ConversationID {
    return this.data.conversationID;
  }
  get topic(): string {
    return this.data.topic;
  }
  get senderInboxID(): B.InboxID {
    return this.data.senderInboxID;
  }
  get sentAt(): B.Timestamp {
    return this.data.sentAt;
  }
  get contentType(): B.ContentTypeID {
    return this.data.contentType;
  }
  get fallback(): string | undefined {
    return this.data.fallback;
  }
  get replyCount(): bigint {
    return this.data.replyCount;
  }
  get reactions(): B.ReactionMessage[] {
    return this.data.reactions;
  }
  get insertedAt(): B.Timestamp {
    return this.data.insertedAt;
  }
  get expiresAt(): B.Timestamp | undefined {
    return this.data.expiresAt;
  }
  get inReplyTo(): B.ReplyParent | undefined {
    return this.data.inReplyTo;
  }

  client(): Client {
    const value = owner(this.session, this.data.clientKey)?.client.deref();
    if (!value) throw closed();
    return value;
  }

  async refresh(): Promise<Message | undefined> {
    const value = await this.client().conversations().getMessageByID(this.id);
    return value === undefined
      ? undefined
      : new Message(value.data, this.session);
  }
  delete(): Promise<B.MessageID> {
    return this.client().conversations().deleteMessage(this.id);
  }
  deleteLocally(): Promise<void> {
    return this.client().conversations().deleteMessageLocally(this.id);
  }
  react(reaction: B.Reaction, options?: B.SendOptions): Promise<B.MessageID> {
    return this.client()
      .conversations()
      .reactToMessage(this.id, reaction, options);
  }
  reply(
    content: string | B.EncodedContent,
    options?: B.SendOptions,
  ): Promise<B.MessageID>;
  reply<T>(
    codec: ContentCodec<T>,
    value: T,
    options?: B.SendOptions,
  ): Promise<B.MessageID>;
  reply(
    content: string | B.EncodedContent | ContentCodec<unknown>,
    valueOrOptions?: unknown,
    options?: B.SendOptions,
  ): Promise<B.MessageID> {
    if (typeof content !== "string" && "encode" in content)
      return this.client()
        .conversations()
        .replyToMessage(this.id, content.encode(valueOrOptions), options);
    if (!isSendOptions(valueOrOptions))
      throw new TypeError("invalid send options");
    const encoded =
      typeof content === "string" ? Pure.encodeText(content) : content;
    return this.client()
      .conversations()
      .replyToMessage(this.id, encoded, valueOrOptions);
  }
  async parent(): Promise<Message | undefined> {
    const id = this.inReplyTo?.id;
    if (id === undefined) return undefined;
    const value = await this.client().conversations().getMessageByID(id);
    return value === undefined
      ? undefined
      : new Message(value.data, this.session);
  }
  conversation(): Promise<B.Conversation | undefined> {
    return this.client().conversations().getByID(this.conversationID);
  }
}

function isSendOptions(value: unknown): value is B.SendOptions | undefined {
  return (
    value === undefined ||
    (value !== null &&
      typeof value === "object" &&
      "optimistic" in value &&
      typeof value.optimistic === "boolean")
  );
}
