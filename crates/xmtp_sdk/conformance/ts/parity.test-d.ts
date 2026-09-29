import type * as NodeHost from "../../../../target/sdk-generated/typescript-napi/index";
// Compare public method parameters and returns, and record fields (plan P62).
// check-parity-signatures.py compares every public declaration. SDK-037 is
// the only removal list.
import type * as Node from "../../../../target/sdk-generated/typescript-napi/xmtp_sdk";
import type * as BrowserHost from "../../../../target/sdk-generated/typescript-wasm/index";
import type * as Browser from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <
    Value,
  >() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Value extends true> = Value;
type Opaque<Value> = Value extends
  | NodeHost.Message
  | BrowserHost.Message
  | Browser.Message
  ? "Message"
  : Value extends NodeHost.Client | BrowserHost.Client
    ? "Client"
    : Value extends Node.Conversation | Browser.Conversation
      ? "Conversation"
      : Value extends Node.ReplyParent | Browser.ReplyParent
        ? "ReplyParent"
        : Value extends Node.StorageLocation | Browser.StorageLocation
          ? "StorageLocation"
          : Value extends Node.ClientOptions | Browser.ClientOptions
            ? "ClientOptions"
            : Value extends Node.StorageLike | Browser.StorageLike
              ? "StorageLike"
              : Value extends Node.ArchivesLike | Browser.ArchivesLike
                ? "ArchivesLike"
                : Value extends Node.MessageContent | Browser.MessageContent
                  ? "MessageContent"
                  : Value extends Node.PublicIdentity | Browser.PublicIdentity
                    ? "PublicIdentity"
                    : Value extends
                          | Node.CreateGroupOptions
                          | Browser.CreateGroupOptions
                      ? "CreateGroupOptions"
                      : Value extends
                            | Node.CreateDmOptions
                            | Browser.CreateDmOptions
                        ? "CreateDmOptions"
                        : Value extends
                              | Node.ListConversationsOptions
                              | Browser.ListConversationsOptions
                          ? "ListConversationsOptions"
                          : Value extends
                                | Node.ConsentEntity
                                | Browser.ConsentEntity
                            ? "ConsentEntity"
                            : Value extends Node.Signature | Browser.Signature
                              ? "Signature"
                              : Value extends
                                    | NodeHost.ConversationId
                                    | BrowserHost.ConversationId
                                ? "ConversationId"
                                : Value extends
                                      | NodeHost.InboxId
                                      | BrowserHost.InboxId
                                  ? "InboxId"
                                  : Value extends
                                        | NodeHost.InstallationId
                                        | BrowserHost.InstallationId
                                    ? "InstallationId"
                                    : Value extends
                                          | NodeHost.MessageId
                                          | BrowserHost.MessageId
                                      ? "MessageId"
                                      : Value extends
                                            | NodeHost.Timestamp
                                            | BrowserHost.Timestamp
                                        ? "Timestamp"
                                        : Value extends
                                              | Node.GroupLike
                                              | Browser.GroupLike
                                          ? "GroupLike"
                                          : Value extends
                                                | Node.DmLike
                                                | Browser.DmLike
                                            ? "DmLike"
                                            : Value extends
                                                  | Node.ConversationsLike
                                                  | Browser.ConversationsLike
                                              ? "ConversationsLike"
                                              : Value extends
                                                    | Node.MessageReaderLike
                                                    | Browser.MessageReaderLike
                                                ? "MessageReaderLike"
                                                : Value extends
                                                      | Node.Signer
                                                      | Browser.Signer
                                                  ? "Signer"
                                                  : Value extends
                                                        | Node.SignatureRequestLike
                                                        | Browser.SignatureRequestLike
                                                    ? "SignatureRequestLike"
                                                    : never;
// Canonical keeps every literal, enum value, union member, optional marker,
// and nested field. It maps each member of a union on its own, so `undefined`
// and other members stay beside an opaque type. Opaque SDK types become their
// name, which also ends the recursion; check-parity-signatures.py compares each
// of them where it is declared.
type Canonical<Value> = 0 extends 1 & Value
  ? "any"
  : Value extends unknown
    ? [Opaque<Value>] extends [never]
      ? Structure<Value>
      : Opaque<Value>
    : never;
type Structure<Value> =
  Value extends Promise<infer Inner>
    ? Promise<Canonical<Inner>>
    : Value extends readonly unknown[]
      ? { [Index in keyof Value]: Canonical<Value[Index]> }
      : Value extends Map<infer Key, infer Inner>
        ? Map<Canonical<Key>, Canonical<Inner>>
        : Value extends Set<infer Inner>
          ? Set<Canonical<Inner>>
          : Value extends (...args: infer Inputs) => infer Output
            ? { inputs: Canonical<Inputs>; output: Canonical<Output> }
            : Value extends object
              ? {
                  [
                    Key in keyof Value as Key extends symbol ? never : Key
                  ]: Canonical<Value[Key]>;
                }
              : Value extends number
                ? ["number", `${Value}`]
                : Value extends string
                  ? ["string", `${Value}`]
                  : Value extends bigint
                    ? ["bigint", `${Value}`]
                    : Value;
type MismatchKeys<Native, Web, Removed extends PropertyKey = never> = {
  [Key in Exclude<keyof Native, Removed> & keyof Web]: Equal<
    Canonical<Native[Key]>,
    Canonical<Web[Key]>
  > extends true
    ? never
    : Key;
}[Exclude<keyof Native, Removed> & keyof Web];
type SameMethods<Native, Web, Removed extends PropertyKey = never> =
  Equal<Exclude<keyof Native, Removed>, keyof Web> extends true
    ? [MismatchKeys<Native, Web, Removed>] extends [never]
      ? true
      : false
    : false;
// Expand the record itself: Opaque would reduce a whole record to its name.
type SameFields<Native, Web> =
  Equal<keyof Native, keyof Web> extends true
    ? Equal<Structure<Native>, Structure<Web>>
    : false;

// check-parity-signatures.py compares every export of both entrypoints, value
// and type-only, and holds the SDK-037 list. This type test compares method
// and record shapes structurally.
type PublicNodeExports =
  keyof typeof import("../../../../target/sdk-generated/typescript-napi/index");
export type QueuedLogSinkIsInternal = Assert<
  Equal<"setLogSinkQueued" extends PublicNodeExports ? true : false, false>
>;

export type BackendParity = Assert<
  SameMethods<Node.BackendLike, Browser.BackendLike>
>;
export type MessageReaderParity = Assert<
  SameMethods<Node.MessageReaderLike, Browser.MessageReaderLike>
>;
export type GroupParity = Assert<
  SameMethods<Node.GroupLike, Browser.GroupLike>
>;
export type DmParity = Assert<SameMethods<Node.DmLike, Browser.DmLike>>;
export type ArchivesParity = Assert<
  SameMethods<
    Node.ArchivesLike,
    Browser.ArchivesLike,
    "exportToFile" | "importFromFile" | "metadataFromFile"
  >
>;
export type ConversationsParity = Assert<
  SameMethods<Node.ConversationsLike, Browser.ConversationsLike>
>;
export type DiagnosticsParity = Assert<
  SameMethods<Node.DiagnosticsLike, Browser.DiagnosticsLike>
>;
export type PreferencesParity = Assert<
  SameMethods<Node.PreferencesLike, Browser.PreferencesLike>
>;
export type StorageParity = Assert<
  SameMethods<Node.StorageLike, Browser.StorageLike, "delete_" | "reconnect">
>;
export type BrowserStorageOmitsReconnect = Assert<
  Equal<"reconnect" extends keyof Browser.StorageLike ? true : false, false>
>;
export type BrowserBridgeStorageOmitsReconnect = Assert<
  Equal<
    "reconnect" extends keyof ReturnType<BrowserHost.Client["storage"]>
      ? true
      : false,
    false
  >
>;
export type SignatureRequestParity = Assert<
  SameMethods<Node.SignatureRequestLike, Browser.SignatureRequestLike>
>;
export type ClientParity = Assert<
  SameMethods<
    Node.ClientLike,
    Browser.ClientLike,
    "disableNotifications" | "enableNotifications" | "notificationState"
  >
>;
type MessageActions =
  | "refresh"
  | "delete"
  | "deleteLocally"
  | "react"
  | "reply"
  | "parent"
  | "conversation"
  | "client";
export type MessageActionParity = Assert<
  SameMethods<
    Pick<NodeHost.Message, MessageActions>,
    Pick<BrowserHost.Message, MessageActions>
  >
>;

// Check records passed across the bridge as well as object method names.
export type ClientOptionsParity = Assert<
  SameFields<
    Omit<Node.ClientOptions, "storage"> & {
      storage: Omit<Node.StorageOptions, "encryptionKey">;
    },
    Browser.ClientOptions
  >
>;
export type EncodedContentParity = Assert<
  SameFields<Node.EncodedContent, Browser.EncodedContent>
>;
export type MessageDataParity = Assert<
  SameFields<Node.MessageData, Browser.MessageData>
>;
export type StorageOptionsParity = Assert<
  SameFields<Omit<Node.StorageOptions, "encryptionKey">, Browser.StorageOptions>
>;
