import type { Message as BoundMessage } from "./ts/message";
import type { Message } from "./ts/public/message";
// Type view for linting the maintained runtime before generation. The
// generator writes the real public values from the binding metadata.
import type * as B from "./xmtp_sdk";

export type InboxId = string;
export type InstallationId = string;
export type ConversationId = string;
export type MessageId = string;
export type DeliveryCursor = string;
export type MessageKind = "application" | "membershipChange";
export type DeliveryStatus = "unpublished" | "published" | "failed";
export type ContentTypeId = {
  readonly authorityId: string;
  readonly typeId: string;
  readonly versionMajor: number;
  readonly versionMinor: number;
};
export type EncodedContent = {
  readonly type: ContentTypeId;
  readonly content: Uint8Array;
  readonly fallback?: string;
};
export type MessageBody =
  | { readonly kind: "text"; readonly value: string }
  | {
      readonly kind: "custom";
      readonly encoded: EncodedContent;
      readonly rawBytes: Uint8Array;
      readonly value?: unknown;
      readonly error?: ErrorDetails;
    }
  | {
      readonly kind: "unknown";
      readonly encoded?: EncodedContent;
      readonly rawBytes: Uint8Array;
      readonly error: ErrorDetails;
    };
export type MessageContent =
  | { readonly kind: "text"; readonly value: string }
  | {
      readonly kind: "reply";
      readonly referenceId: MessageId;
      readonly body: MessageBody;
    }
  | {
      readonly kind: "custom";
      readonly encoded: EncodedContent;
      readonly rawBytes: Uint8Array;
      readonly value?: unknown;
      readonly error?: ErrorDetails;
    }
  | {
      readonly kind: "unknown";
      readonly encoded?: EncodedContent;
      readonly rawBytes: Uint8Array;
      readonly error: ErrorDetails;
    };
export type Reaction = { readonly content: string };
export type ReactionMessage = { readonly id: MessageId };
export type ReplyParent = { readonly id: MessageId };
export type SendOptions = { readonly shouldPush?: boolean };
export type PublicIdentity = {
  readonly kind: "ethereum" | "passkey";
  readonly identifier: string;
};
export interface Signer {
  identity(): Promise<PublicIdentity>;
}
export type BackendOptions = { readonly url: string };
export type BackendSource = BackendOptions;
export type StorageOptions = { readonly location: "default" | "inMemory" };
export type ClientOptions = {
  readonly backend?: BackendSource;
  readonly storage: StorageOptions;
};
export type InboxState = { readonly inboxId: InboxId };
export type KeyPackageStatus = { readonly valid: boolean };
export type MessageMetadataEntry = { readonly cursor: bigint };
export type ServerConfiguration = { readonly url: string };
export declare class Group {
  id(): ConversationId;
}
export declare class Dm {
  id(): ConversationId;
}
export type Conversation = Group | Dm;
export declare class Conversations {
  getById(id: ConversationId): Promise<Conversation | undefined>;
  getMessageById(id: MessageId): Promise<Message | undefined>;
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
}

export declare abstract class ObjectProjection {
  abstract liftMessage(value: BoundMessage): Message;
  abstract lowerMessage(value: Message): BoundMessage;
}
export declare abstract class ClientMembers {
  get conversations(): Conversations;
}
export declare function attachClientBinding(
  client: ClientMembers,
  binding: B.ClientLike,
): void;
export declare function installProjection(value: ObjectProjection): void;
export declare function currentProjection(): ObjectProjection;

type Lift<Binding, Public> = (
  value: Binding,
  projection: ObjectProjection,
) => Public;
type Lower<Public, Binding> = (
  value: Public,
  projection: ObjectProjection,
) => Binding;

export declare const liftContentTypeId: Lift<object, ContentTypeId>;
export declare const lowerContentTypeId: Lower<ContentTypeId, B.ContentTypeId>;
export declare const liftEncodedContent: Lift<B.EncodedContent, EncodedContent>;
export declare const liftErrorDetails: Lift<B.ErrorDetails, ErrorDetails>;
export declare const lowerEncodedContent: Lower<
  EncodedContent,
  B.EncodedContent
>;
export declare const liftMessageContent: Lift<B.MessageContent, MessageContent>;
export declare const liftMessageBody: Lift<B.MessageBody, MessageBody>;
export declare const liftMessageKind: Lift<string, MessageKind>;
export declare const liftDeliveryStatus: Lift<string, DeliveryStatus>;
export declare const liftReactionMessage: Lift<object, ReactionMessage>;
export declare const liftReplyParent: Lift<
  NonNullable<B.MessageData["inReplyTo"]>,
  ReplyParent
>;
export declare const liftInboxState: Lift<B.InboxState, InboxState>;
export declare const liftKeyPackageStatus: Lift<
  B.KeyPackageStatus,
  KeyPackageStatus
>;
export declare const liftMessageMetadataEntry: Lift<
  B.MessageMetadataEntry,
  MessageMetadataEntry
>;
export declare const liftServerConfiguration: Lift<
  B.ServerConfiguration,
  ServerConfiguration
>;
export declare const lowerBackendSource: Lower<BackendSource, B.BackendSource>;
export declare const lowerClientOptions: Lower<ClientOptions, B.ClientOptions>;
export declare const lowerPublicIdentity: Lower<
  PublicIdentity,
  B.PublicIdentity
>;
export declare const lowerSigner: Lower<Signer, B.Signer>;

export type ErrorDetails = {
  readonly code: string;
  readonly category: string;
  readonly retryable: boolean;
  readonly message: string;
};
export declare class XmtpError extends Error {
  readonly details: ErrorDetails;
  constructor(details: ErrorDetails);
  static readonly CodecEncodeFailed: typeof XmtpError;
  static readonly InvalidArgument: typeof XmtpError;
}
export declare function publicError(error: unknown): unknown;
export declare function isCatalogueContentType(type: ContentTypeId): boolean;

export type ConnectionState =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "failed"
  | "closed";
export type ClientEvent = { readonly kind: string };
export type EventFilter = { readonly kinds?: readonly string[] };
export type LogRecord = { readonly message: string };
export type MessageReaderOptions = { readonly from?: string };
export type ConversationMessageReaderOptions = { readonly from?: string };
export type ConversationReaderOptions = { readonly kind?: "group" | "dm" };
export declare const liftConnectionState: Lift<
  B.ConnectionState,
  ConnectionState
>;
export declare const liftConversation: Lift<B.Conversation, Conversation>;
export declare const liftClientEvent: Lift<B.ClientEvent, ClientEvent>;
export declare const lowerEventFilter: Lower<EventFilter, B.EventFilter>;
export declare const liftLogRecord: Lift<B.LogRecord, LogRecord>;
export declare const lowerMessageReaderOptions: Lower<
  MessageReaderOptions,
  B.MessageReaderOptions
>;
export declare const lowerConversationMessageReaderOptions: Lower<
  ConversationMessageReaderOptions,
  B.ConversationMessageReaderOptions
>;
export declare const lowerConversationReaderOptions: Lower<
  ConversationReaderOptions,
  B.ConversationReaderOptions
>;

type MessageSource = {
  messageReader(
    options: B.ConversationMessageReaderOptions | undefined,
    asyncOptions?: { signal: AbortSignal },
  ): Promise<B.MessageReaderLike>;
};
export declare function unwrapConversations(
  value: Conversations,
): B.ConversationsLike & {
  messageReader(
    options: B.MessageReaderOptions | undefined,
    asyncOptions?: { signal: AbortSignal },
  ): Promise<B.MessageReaderLike>;
};
export declare function unwrapGroup(value: Group): MessageSource;
export declare function unwrapDm(value: Dm): MessageSource;

// Standard codec values.
export type StandardContent = { readonly kind: string };
export type Attachment = { readonly filename?: string };
export type RemoteAttachment = { readonly url: string };
export type MultiRemoteAttachment = { readonly attachments: object[] };
export type TransactionReference = { readonly reference: string };
export type WalletSendCalls = { readonly version: string };
export type Actions = { readonly id: string };
export type Intent = { readonly id: string };
export type GroupUpdated = { readonly initiatedByInboxId: string };
export type LeaveRequest = { readonly authenticationNote?: Uint8Array };
export declare const liftStandardContent: Lift<
  B.StandardContent,
  StandardContent
>;
export declare const lowerStandardContent: Lower<
  StandardContent,
  B.StandardContent
>;
export declare const liftAttachment: Lift<B.Attachment, Attachment>;
export declare const lowerAttachment: Lower<Attachment, B.Attachment>;
export declare const liftRemoteAttachment: Lift<
  B.RemoteAttachment,
  RemoteAttachment
>;
export declare const lowerRemoteAttachment: Lower<
  RemoteAttachment,
  B.RemoteAttachment
>;
export declare const liftMultiRemoteAttachment: Lift<
  B.MultiRemoteAttachment,
  MultiRemoteAttachment
>;
export declare const lowerMultiRemoteAttachment: Lower<
  MultiRemoteAttachment,
  B.MultiRemoteAttachment
>;
export declare const liftTransactionReference: Lift<
  B.TransactionReference,
  TransactionReference
>;
export declare const lowerTransactionReference: Lower<
  TransactionReference,
  B.TransactionReference
>;
export declare const liftWalletSendCalls: Lift<
  B.WalletSendCalls,
  WalletSendCalls
>;
export declare const lowerWalletSendCalls: Lower<
  WalletSendCalls,
  B.WalletSendCalls
>;
export declare const liftActions: Lift<B.Actions, Actions>;
export declare const lowerActions: Lower<Actions, B.Actions>;
export declare const liftIntent: Lift<B.Intent, Intent>;
export declare const lowerIntent: Lower<Intent, B.Intent>;
export declare const liftGroupUpdated: Lift<B.GroupUpdated, GroupUpdated>;
export declare const lowerGroupUpdated: Lower<GroupUpdated, B.GroupUpdated>;
export declare const liftLeaveRequest: Lift<B.LeaveRequest, LeaveRequest>;
export declare const lowerLeaveRequest: Lower<LeaveRequest, B.LeaveRequest>;
