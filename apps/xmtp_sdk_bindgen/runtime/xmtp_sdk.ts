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
  backend: object;
  deviceSync: boolean;
};
export type PublicIdentity = object;
export type Signer = object;
export type ConversationsLike = object;

export interface ClientLike {
  clientKey(): bigint;
  inboxID(): InboxID;
  installationID(): InstallationID;
  conversations(): ConversationsLike;
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
