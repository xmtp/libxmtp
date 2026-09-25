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
  senderInboxID: InboxID;
  sentAt: Timestamp;
  kind: string;
  deliveryStatus: string;
  contentType: object;
  fallback?: string;
  content: object;
};

export enum ErrorCategory {
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
export type LogRecord = {
  level: number;
  target: string;
  message: string;
  fields: Map<string, string>;
  timestampNs: bigint;
  droppedRecords: bigint;
};
export type ConversationsLike = object;
export type ClientEvent = object;
export type EventFilter = object;
export declare const ListenerError: { Failed: new () => Error };
export interface EventReaderLike {
  next(options?: { signal: AbortSignal }): Promise<ClientEvent | undefined>;
  end(): Promise<void>;
}

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
  events(filter: EventFilter): Promise<EventReaderLike>;
  startListener(
    filter: EventFilter,
    listener: { onEvent(event: ClientEvent): Promise<void> },
  ): Promise<bigint>;
  stopListener(id: bigint): Promise<void>;
  end(): Promise<void>;
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
