import type { Message as BoundMessage } from "./ts/message";
import type { Message } from "./ts/public/message";
// Type view for linting the maintained runtime before generation. The
// generator writes the real public values from the binding metadata.
import type * as B from "./xmtp_sdk";

export type InboxId = string;
export type InstallationId = string;
export type ConversationId = string;
export type MessageId = string;
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
};
export type MessageBody =
  | { readonly kind: "text"; readonly value: string }
  | {
      readonly kind: "custom";
      readonly encoded: EncodedContent;
      readonly value?: unknown;
      readonly error?: string;
    }
  | { readonly kind: "unknown"; readonly encoded: EncodedContent };
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
      readonly error?: string;
    }
  | {
      readonly kind: "unknown";
      readonly encoded: EncodedContent;
      readonly rawBytes: Uint8Array;
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
  protected abstract bindingClient(): B.ClientLike;
  conversations(): Conversations;
}
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
