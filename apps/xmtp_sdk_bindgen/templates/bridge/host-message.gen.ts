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
  readonly type: B.ContentTypeId;
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
  if (Reflect.get(options.storage, "encryptionKey") !== undefined)
    throw B.XmtpError.InvalidInput.new({
      code: "InvalidInput",
      category: B.ErrorCategory.Input,
      retryable: false,
      message: "browser storage does not support encryptionKey",
    });
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

// A client that is collected without `end()` must not keep its codecs. Remove
// the entry only if its client is gone: `end()` can already have removed it,
// and a later client can use the same key.
const collectedClients = new FinalizationRegistry<{
  session: MainSession;
  key: bigint;
}>(({ session, key }) => {
  if (owners.get(session)?.get(key)?.client.deref() === undefined)
    unregisterClient(session, key);
});

export function registerClient(
  session: MainSession,
  client: Client,
  codecs: readonly AnyCodec[],
): void {
  const key = client.clientKey();
  const entries = owners.get(session) ?? new Map<bigint, Owner>();
  entries.set(key, {
    client: new WeakRef(client),
    codecs: new Map(codecs.map((codec) => [codecKey(codec.type), codec])),
  });
  owners.set(session, entries);
  collectedClients.register(client, { session, key });
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
  inner: { referenceId: B.MessageId; body: LiftedReplyBody };
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

// The worker sends each message with the content that Rust decoded, with
// bounded decompression and the Unknown fallback for bytes that fail to
// decode. The host keeps that content and only lifts custom content with the
// client's codecs. The encoded bytes are not decoded again here.
function decodeContent(
  session: MainSession,
  key: bigint,
  content: B.MessageContent,
): HostContent {
  if (content.tag === B.MessageContent_Tags.Custom) {
    const entry = owner(session, key);
    return liftCustomContent(
      content,
      entry !== undefined,
      decodeCustom(entry?.codecs, content.inner.encoded),
    );
  }
  if (content.tag !== B.MessageContent_Tags.Reply) return content;
  return {
    tag: B.MessageContent_Tags.Reply,
    inner: {
      referenceId: content.inner.referenceId,
      body: decodeBody(session, key, content.inner.body),
    },
  };
}

function decodeBody(
  session: MainSession,
  key: bigint,
  body: B.MessageBody,
): LiftedReplyBody {
  if (body.tag !== B.MessageBody_Tags.Custom) return body;
  const entry = owner(session, key);
  return liftCustomBody(
    body,
    entry !== undefined,
    decodeCustom(entry?.codecs, body.inner.encoded),
  );
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
    this.content = decodeContent(session, data.clientKey, data.content);
    this.inReplyToContent = data.inReplyTo
      ? decodeBody(session, data.clientKey, data.inReplyTo.content)
      : undefined;
    this.replyContent =
      this.content.tag === B.MessageContent_Tags.Reply
        ? this.content.inner.body
        : undefined;
  }

  get deliveryCursor(): string | null {
    return this.data.deliveryCursor ?? null;
  }

  get conversationId(): B.ConversationId {
    return this.data.conversationId;
  }
  get topic(): string {
    return this.data.topic;
  }
  get senderInboxId(): B.InboxId {
    return this.data.senderInboxId;
  }
  get sentAt(): B.Timestamp {
    return this.data.sentAt;
  }
  get contentType(): B.ContentTypeId {
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
    const value = await this.client().conversations().getMessageById(this.id);
    return value === undefined
      ? undefined
      : new Message(value.data, this.session);
  }
  async delete(): Promise<B.MessageId> {
    return this.client().conversations().deleteMessage(this.id);
  }
  async deleteLocally(): Promise<void> {
    return this.client().conversations().deleteMessageLocally(this.id);
  }
  async react(
    reaction: B.Reaction,
    options?: B.SendOptions,
  ): Promise<B.MessageId> {
    return this.client()
      .conversations()
      .reactToMessage(this.id, reaction, options);
  }
  reply(
    content: string | B.EncodedContent,
    options?: B.SendOptions,
  ): Promise<B.MessageId>;
  reply<T>(
    codec: ContentCodec<T>,
    value: T,
    options?: B.SendOptions,
  ): Promise<B.MessageId>;
  async reply(
    content: string | B.EncodedContent | ContentCodec<unknown>,
    valueOrOptions?: unknown,
    options?: B.SendOptions,
  ): Promise<B.MessageId> {
    if (typeof content !== "string" && "encode" in content)
      return this.client()
        .conversations()
        .replyToMessage(
          this.id,
          content.encode(valueOrOptions),
          sendOptions(options),
        );
    const checked = sendOptions(valueOrOptions);
    const encoded =
      typeof content === "string" ? Pure.encodeText(content) : content;
    return this.client()
      .conversations()
      .replyToMessage(this.id, encoded, checked);
  }
  async parent(): Promise<Message | undefined> {
    const id = this.inReplyTo?.id;
    if (id === undefined) return undefined;
    const value = await this.client().conversations().getMessageById(id);
    return value === undefined
      ? undefined
      : new Message(value.data, this.session);
  }
  async conversation(): Promise<B.Conversation | undefined> {
    return this.client().conversations().getById(this.conversationId);
  }
}

// Rust gives every SendOptions field a default, so a caller can leave out any
// field. Check only the fields that are present, then fill in the defaults.
function sendOptions(value: unknown): B.SendOptions | undefined {
  if (value === undefined) return undefined;
  if (value === null || typeof value !== "object" || Array.isArray(value))
    throw new TypeError("invalid send options");
  const shouldPush: unknown = Reflect.get(value, "shouldPush");
  const optimistic: unknown = Reflect.get(value, "optimistic");
  const idempotencyKey: unknown = Reflect.get(value, "idempotencyKey");
  const compression: unknown = Reflect.get(value, "compression");
  if (
    (shouldPush !== undefined && typeof shouldPush !== "boolean") ||
    (optimistic !== undefined && typeof optimistic !== "boolean") ||
    (idempotencyKey !== undefined && typeof idempotencyKey !== "string") ||
    (compression !== undefined && !isCompression(compression))
  )
    throw new TypeError("invalid send options");
  const defaults = B.SendOptions.create({});
  return B.SendOptions.create({
    shouldPush,
    optimistic: optimistic ?? defaults.optimistic,
    idempotencyKey,
    compression,
  });
}

function isCompression(value: unknown): value is B.Compression {
  return typeof value === "number" && value in B.Compression;
}
