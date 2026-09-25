// Type view for linting the maintained runtime before generation. The generated
// package resolves this import to its own xmtp_sdk.ts instead.
import type {
  ConversationID,
  InboxID,
  InstallationID,
  MessageID,
  Timestamp,
} from "./ts/ids";
import type { Message } from "./ts/message";

export type MessageData = {
  id: MessageID;
  clientKey: bigint;
  conversationID: ConversationID;
  topic: string;
  senderInboxID: InboxID;
  sentAt: Timestamp;
  insertedAt: Timestamp;
  expiresAt?: Timestamp;
  kind: string;
  deliveryStatus: string;
  contentType: object;
  fallback?: string;
  encoded: EncodedContent;
  content: MessageContent;
  replyCount: bigint;
  reactions: object[];
  inReplyTo?: { id: MessageID; content: MessageBody };
};
export type ContentTypeID = {
  authorityID: string;
  typeID: string;
  versionMajor: number;
  versionMinor: number;
};
export type EncodedContent = { type: ContentTypeID; content: ArrayBuffer };
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
export function standardContentType(_kind: StandardContentKind): ContentTypeID {
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
export type MessageBody = {
  tag: MessageBody_Tags;
  inner: { encoded: EncodedContent };
};
export const MessageBody = {
  Unknown: {
    new(inner: { encoded: EncodedContent }): MessageBody {
      return { tag: MessageBody_Tags.Unknown, inner };
    },
  },
};
export type MessageContent =
  | {
      tag: MessageContent_Tags.Reply;
      inner: { referenceID: MessageID; body: MessageBody };
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
      inner: { encoded: EncodedContent; rawBytes: ArrayBuffer };
    };
export const MessageContent = {
  Unknown: {
    new(inner: {
      encoded: EncodedContent;
      rawBytes: ArrayBuffer;
    }): MessageContent {
      return { tag: MessageContent_Tags.Unknown, inner };
    },
  },
};
export type Reaction = object;
export type SendOptions = object;

export enum ErrorCategory {
  Input,
  Lifecycle,
}

export type ErrorDetails = {
  code: string;
  category: ErrorCategory;
  retryable: boolean;
  message: string;
};

export declare const XmtpError: {
  ClientClosed: new (details: ErrorDetails) => Error;
  InvalidArgument: new (details: ErrorDetails) => Error;
};

export enum StorageLocation_Tags {
  Default = "Default",
  Directory = "Directory",
}

export type StorageLocation = { tag: StorageLocation_Tags };
export declare const StorageLocation: {
  Directory: new (directory: string) => StorageLocation;
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
export type CanMessageEntry = { identity: PublicIdentity; canMessage: boolean };
export type InboxState = object;
export type KeyPackageStatusEntry = object;
export type MessageMetadataEntry = object;
export type ServerConfiguration = object;
export type Conversation = object;
export type LogRecord = {
  level: number;
  target: string;
  message: string;
  fields: Map<string, string>;
  timestampNs: bigint;
  droppedRecords: bigint;
};
export type ConversationsLike = {
  getMessageByID(id: MessageID): Promise<Message | undefined>;
  getByID(id: ConversationID): Promise<Conversation | undefined>;
  deleteMessage(id: MessageID): Promise<MessageID>;
  deleteMessageLocally(id: MessageID): Promise<void>;
  reactToMessage(
    id: MessageID,
    reaction: Reaction,
    options?: SendOptions,
  ): Promise<MessageID>;
  replyToMessage(
    id: MessageID,
    content: EncodedContent,
    options?: SendOptions,
  ): Promise<MessageID>;
};
export declare function encodeText(text: string): EncodedContent;

export declare function fetchServerConfiguration(
  options: BackendOptions,
): Promise<ServerConfiguration>;
export declare function canMessageWithBackend(
  backend: BackendLike,
  identities: PublicIdentity[],
): Promise<CanMessageEntry[]>;
export declare function inboxIdForWithBackend(
  backend: BackendLike,
  identity: PublicIdentity,
): Promise<InboxID>;
export declare function inboxStatesWithBackend(
  backend: BackendLike,
  ids: InboxID[],
): Promise<InboxState[]>;
export declare function keyPackageStatusesWithBackend(
  backend: BackendLike,
  ids: InstallationID[],
): Promise<KeyPackageStatusEntry[]>;
export declare function newestMessageMetadataWithBackend(
  backend: BackendLike,
  ids: ConversationID[],
): Promise<MessageMetadataEntry[]>;
export declare function revokeInstallationsWithBackend(
  backend: BackendLike,
  signer: Signer,
  inboxID: InboxID,
  ids: InstallationID[],
): Promise<void>;
export declare function isAddressAuthorizedWithBackend(
  backend: BackendLike,
  inboxID: InboxID,
  address: string,
): Promise<boolean>;
export declare function isInstallationAuthorizedWithBackend(
  backend: BackendLike,
  inboxID: InboxID,
  installationID: InstallationID,
): Promise<boolean>;
export declare function verifySignedWithPublicKey(
  text: string,
  signature: ArrayBuffer,
  publicKey: ArrayBuffer,
): Promise<boolean>;

export interface ClientLike {
  clientKey(): bigint;
  inboxID(): InboxID;
  installationID(): InstallationID;
  conversations(): ConversationsLike;
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
    inboxID?: InboxID,
  ): Promise<ClientLike>;
};

export interface MessageReaderLike {
  next(options?: { signal: AbortSignal }): Promise<Message | undefined>;
  end(): Promise<void>;
}

export type Conversation = object;
export enum ConnectionState {
  Connecting,
  Connected,
  Reconnecting,
  Failed,
  Closed,
}
