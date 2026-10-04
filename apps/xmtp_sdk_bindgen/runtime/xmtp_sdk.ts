// Type view for linting the maintained runtime before generation. The generated
// package resolves this import to its own xmtp_sdk.ts instead.
import type { Timestamp } from "./ts/ids";
import type { Message } from "./ts/message";

export type ConversationId = string;
export type InboxId = string;
export type InstallationId = string;
export type MessageId = string;
export type MessageKind = string;
export type DeliveryStatus = string;

export type MessageData = {
  id: MessageId;
  clientKey: bigint;
  deliveryCursor?: string;
  conversationId: ConversationId;
  topic: string;
  senderInboxId: InboxId;
  sentAt: Timestamp;
  insertedAt: Timestamp;
  expiresAt?: Timestamp;
  kind: string;
  deliveryStatus: string;
  rawBytes: ArrayBuffer;
  contentType?: ContentTypeId;
  fallback?: string;
  encoded?: EncodedContent;
  content: MessageContent;
  replyCount: bigint;
  reactions: object[];
  inReplyTo?: { id: MessageId; content: MessageBody };
};
export type ContentTypeId = {
  authorityId: string;
  typeId: string;
  versionMajor: number;
  versionMinor: number;
};
export type EncodedContent = {
  type: ContentTypeId;
  content: ArrayBuffer;
  fallback?: string;
};
export type Attachment = object;
export type RemoteAttachment = object;
export type MultiRemoteAttachment = object;
export type TransactionReference = object;
export type WalletSendCalls = object;
export type Actions = object;
export type Intent = object;
export type GroupUpdated = object;
export type LeaveRequest = object;
export enum StandardContentKind {
  Text,
  Markdown,
  ReadReceipt,
  Reaction,
  Attachment,
  RemoteAttachment,
  MultiRemoteAttachment,
  TransactionReference,
  WalletSendCalls,
  Actions,
  Intent,
  Reply,
  GroupUpdated,
  DeleteMessage,
  LeaveRequest,
}
export enum StandardContent_Tags {
  Text = "Text",
  Markdown = "Markdown",
  ReadReceipt = "ReadReceipt",
  Reaction = "Reaction",
  Attachment = "Attachment",
  RemoteAttachment = "RemoteAttachment",
  MultiRemoteAttachment = "MultiRemoteAttachment",
  TransactionReference = "TransactionReference",
  WalletSendCalls = "WalletSendCalls",
  Actions = "Actions",
  Intent = "Intent",
  Reply = "Reply",
  GroupUpdated = "GroupUpdated",
  DeleteMessage = "DeleteMessage",
  LeaveRequest = "LeaveRequest",
}
export type StandardContent = {
  tag: StandardContent_Tags;
  inner: readonly unknown[] | object;
};
export const StandardContent = {
  Text: class {
    readonly tag = StandardContent_Tags.Text;
    readonly inner: readonly [string];
    constructor(value: string) {
      this.inner = [value];
    }
  },
  Markdown: class {
    readonly tag = StandardContent_Tags.Markdown;
    readonly inner: readonly [string];
    constructor(value: string) {
      this.inner = [value];
    }
  },
  ReadReceipt: class {
    readonly tag = StandardContent_Tags.ReadReceipt;
    readonly inner: readonly [] = [];
  },
  Attachment: class {
    readonly tag = StandardContent_Tags.Attachment;
    readonly inner: readonly [Attachment];
    constructor(value: Attachment) {
      this.inner = [value];
    }
  },
  RemoteAttachment: class {
    readonly tag = StandardContent_Tags.RemoteAttachment;
    readonly inner: readonly [RemoteAttachment];
    constructor(value: RemoteAttachment) {
      this.inner = [value];
    }
  },
  MultiRemoteAttachment: class {
    readonly tag = StandardContent_Tags.MultiRemoteAttachment;
    readonly inner: readonly [MultiRemoteAttachment];
    constructor(value: MultiRemoteAttachment) {
      this.inner = [value];
    }
  },
  TransactionReference: class {
    readonly tag = StandardContent_Tags.TransactionReference;
    readonly inner: readonly [TransactionReference];
    constructor(value: TransactionReference) {
      this.inner = [value];
    }
  },
  WalletSendCalls: class {
    readonly tag = StandardContent_Tags.WalletSendCalls;
    readonly inner: readonly [WalletSendCalls];
    constructor(value: WalletSendCalls) {
      this.inner = [value];
    }
  },
  Actions: class {
    readonly tag = StandardContent_Tags.Actions;
    readonly inner: readonly [Actions];
    constructor(value: Actions) {
      this.inner = [value];
    }
  },
  Intent: class {
    readonly tag = StandardContent_Tags.Intent;
    readonly inner: readonly [Intent];
    constructor(value: Intent) {
      this.inner = [value];
    }
  },
  GroupUpdated: class {
    readonly tag = StandardContent_Tags.GroupUpdated;
    readonly inner: readonly [GroupUpdated];
    constructor(value: GroupUpdated) {
      this.inner = [value];
    }
  },
  LeaveRequest: class {
    readonly tag = StandardContent_Tags.LeaveRequest;
    readonly inner: readonly [LeaveRequest];
    constructor(value: LeaveRequest) {
      this.inner = [value];
    }
  },
};
export function catalogueContentTypeShouldPush(
  _contentType: ContentTypeId,
): boolean {
  throw new Error("lint only");
}
export function standardContentType(_kind: StandardContentKind): ContentTypeId {
  throw new Error("lint only");
}
export function encodeStandard(_value: StandardContent): EncodedContent {
  throw new Error("lint only");
}
export function decodeStandard(_encoded: EncodedContent): StandardContent {
  throw new Error("lint only");
}
export enum MessageContent_Tags {
  Text = "Text",
  Reply = "Reply",
  Custom = "Custom",
  Unknown = "Unknown",
}
export enum MessageBody_Tags {
  Text = "Text",
  Custom = "Custom",
  Unknown = "Unknown",
}
type UnknownContent = {
  encoded?: EncodedContent;
  rawBytes: ArrayBuffer;
  error: ErrorDetails;
};
export type MessageBody =
  | { tag: MessageBody_Tags.Text; inner: [string] }
  | {
      tag: MessageBody_Tags.Custom;
      inner: { encoded: EncodedContent; rawBytes: ArrayBuffer };
    }
  | { tag: MessageBody_Tags.Unknown; inner: UnknownContent };
export const MessageBody = {
  Unknown: {
    new(
      inner: UnknownContent,
    ): Extract<MessageBody, { tag: MessageBody_Tags.Unknown }> {
      return { tag: MessageBody_Tags.Unknown, inner };
    },
  },
};
export type MessageContent =
  | {
      tag: MessageContent_Tags.Reply;
      inner: { referenceId: MessageId; body: MessageBody };
    }
  | {
      tag: MessageContent_Tags.Text;
      inner: { encoded: EncodedContent };
    }
  | {
      tag: MessageContent_Tags.Custom;
      inner: { encoded: EncodedContent; rawBytes: ArrayBuffer };
    }
  | {
      tag: MessageContent_Tags.Unknown;
      inner: UnknownContent;
    };
export const MessageContent = {
  Unknown: {
    new(
      inner: UnknownContent,
    ): Extract<MessageContent, { tag: MessageContent_Tags.Unknown }> {
      return { tag: MessageContent_Tags.Unknown, inner };
    },
  },
};
export type Reaction = object;
export type SendOptions = object;

export enum ErrorCategory {
  Input,
  Network,
  Storage,
  Identity,
  Conversation,
  Callback,
  Lifecycle,
  Configuration,
  Notification,
  Stream,
  Unknown,
}

export type ErrorDetails = {
  code: string;
  category: ErrorCategory;
  retryable: boolean;
  message: string;
  streamFailure?: unknown;
};

export declare const XmtpError: {
  ClientClosed: new (details: ErrorDetails) => Error;
  InvalidArgument: new (details: ErrorDetails) => Error;
  IdentityNotFound: new (details: ErrorDetails) => Error;
};

export enum StorageLocation_Tags {
  Default = "Default",
  Directory = "Directory",
}

export type StorageLocation = { tag: StorageLocation_Tags };
export declare const StorageLocation: {
  Directory: new (inner: { directory: string }) => StorageLocation;
};

export type ClientOptions = {
  storage: {
    location: StorageLocation;
    label?: string;
    encryptionKey?: ArrayBuffer;
  };
  backend?: BackendSource;
  deviceSync: boolean;
};
export type PublicIdentity = object;
export type Signer = object;
export type BackendLike = object;
export type BackendOptions = object;
export type BackendSource = object;
export declare const BackendSource: {
  Options: new (options: BackendOptions) => object;
  Connected: new (backend: BackendLike) => object;
};
export type InboxState = object;
export type KeyPackageStatus = object;
export type MessageMetadataEntry = object;
export type ServerConfiguration = object;
export type Conversation = object;
export enum ConsentState {
  Unknown,
  Allowed,
  Denied,
}
export enum ConversationKind {
  Group,
  Dm,
}
export type ConversationReaderOptions = {
  kind?: ConversationKind;
  consentStates?: ConsentState[];
};
export interface ConversationReaderLike {
  next(options?: { signal: AbortSignal }): Promise<Conversation | undefined>;
  end(): Promise<void>;
  connectionState(): Promise<ConnectionState>;
  connectionStateChanged(previous: ConnectionState): Promise<ConnectionState>;
}
export type LogRecord = {
  level: number;
  target: string;
  message: string;
  fields: Map<string, string>;
  timestamp: Timestamp;
  droppedRecords: bigint;
};
export declare function setLogSink(sink?: {
  log(record: LogRecord): Promise<void>;
}): Promise<void>;
export declare function sdkLogSinkHandoff(): boolean;
export declare const LogSinkError: {
  Failed: new (fields: { reason: string }) => Error;
};
export type ClientEvent = object;
export type EventFilter = object;
export declare const ListenerError: { Failed: new () => Error };
export interface EventReaderLike {
  next(options?: { signal: AbortSignal }): Promise<ClientEvent | undefined>;
  end(): Promise<void>;
}
export type ConversationsLike = {
  conversationReader(
    options: ConversationReaderOptions | undefined,
    asyncOptions?: { signal: AbortSignal },
  ): Promise<ConversationReaderLike>;
  getMessageById(id: MessageId): Promise<Message | undefined>;
  getById(id: ConversationId): Promise<Conversation | undefined>;
  deleteMessage(id: MessageId): Promise<MessageId>;
  deleteMessageLocally(id: MessageId): Promise<void>;
  reactToMessage(
    id: MessageId,
    reaction: Reaction,
    options?: SendOptions,
  ): Promise<MessageId>;
  replyToMessage(
    id: MessageId,
    content: EncodedContent,
    options?: SendOptions,
  ): Promise<MessageId>;
};
export declare function encodeText(text: string): EncodedContent;

export declare function fetchServerConfiguration(
  options: BackendOptions,
): Promise<ServerConfiguration>;
export declare function canMessageWithBackend(
  backend: BackendLike,
  identities: PublicIdentity[],
): Promise<Map<string, boolean>>;
export declare function inboxIdForWithBackend(
  backend: BackendLike,
  identity: PublicIdentity,
): Promise<InboxId>;
export declare function inboxStatesWithBackend(
  backend: BackendLike,
  ids: InboxId[],
): Promise<InboxState[]>;
export declare function keyPackageStatusesWithBackend(
  backend: BackendLike,
  ids: InstallationId[],
): Promise<Map<string, KeyPackageStatus>>;
export declare function newestMessageMetadataWithBackend(
  backend: BackendLike,
  ids: ConversationId[],
): Promise<Map<string, MessageMetadataEntry>>;
export declare function revokeInstallationsWithBackend(
  backend: BackendLike,
  signer: Signer,
  inboxId: InboxId,
  ids: InstallationId[],
): Promise<void>;
export declare function isAddressAuthorizedWithBackend(
  backend: BackendLike,
  inboxId: InboxId,
  address: string,
): Promise<boolean>;
export declare function isInstallationAuthorizedWithBackend(
  backend: BackendLike,
  inboxId: InboxId,
  installationId: InstallationId,
): Promise<boolean>;
export declare function verifySignedWithPublicKey(
  text: string,
  signature: ArrayBuffer,
  publicKey: ArrayBuffer,
): Promise<boolean>;

export interface ClientLike {
  clientKey(): bigint;
  inboxId(): InboxId;
  installationId(): InstallationId;
  conversations(): ConversationsLike;
  events(filter: EventFilter): Promise<EventReaderLike>;
  startListener(
    filter: EventFilter,
    listener: { onEvent(event: ClientEvent): Promise<void> },
  ): Promise<bigint>;
  stopListener(id: bigint): Promise<void>;
  storage(): StorageLike;
  end(): Promise<void>;
}

export interface StorageLike {
  path(): Promise<string | undefined>;
}

export declare const Client: {
  create(signer: Signer, options: ClientOptions): Promise<ClientLike>;
  build(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId?: InboxId,
  ): Promise<ClientLike>;
};

export interface MessageReaderLike {
  next(options?: { signal: AbortSignal }): Promise<Message | undefined>;
  end(): Promise<void>;
}

export enum ConnectionState {
  Connecting,
  Connected,
  Reconnecting,
  Failed,
  Closed,
}

export type MessageReaderOptions = {
  conversationKind?: ConversationKind;
  consentStates?: ConsentState[];
  from?: string;
};
export type ConversationMessageReaderOptions = { from?: string };
