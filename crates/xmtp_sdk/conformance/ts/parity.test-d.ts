// The object interfaces are the common public Rust surface on Node and WASM.
// The worker Message is data-only, so object methods compare name, arity, and
// sync/async form. Wire records use full bidirectional assignment below.
// SDK-037 removes the listed native methods. Task 19 adds catchUpToLive.
import type * as Node from "../../../../target/sdk-generated/typescript-napi/xmtp_sdk";
import type * as Browser from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <
    Value,
  >() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Value extends true> = Value;
type MethodShape<Value> = Value extends (...args: infer Inputs) => infer Output
  ? {
      arity: Inputs["length"];
      async: Output extends Promise<unknown> ? true : false;
    }
  : "field";
type Shapes<Value> = { [Key in keyof Value]: MethodShape<Value[Key]> };
type SameMethods<Native, Web, Removed extends PropertyKey = never> =
  Equal<Exclude<keyof Native, Removed>, keyof Web> extends true
    ? Equal<
        Shapes<Pick<Native, Exclude<keyof Native, Removed>>>,
        Shapes<Web>
      > extends true
      ? true
      : false
    : false;
type SameFields<Native, Web> =
  Equal<keyof Native, keyof Web> extends true
    ? Web extends Native
      ? Native extends Web
        ? true
        : false
      : false
    : false;

// Browser joins worker WASM and main-thread pure WASM exports.
type NativeExports =
  keyof typeof import("../../../../target/sdk-generated/typescript-napi/xmtp_sdk");
type BrowserExports =
  | keyof typeof import("../../../../target/sdk-generated/typescript-wasm/xmtp_sdk")
  | keyof typeof import("../../../../target/sdk-generated/typescript-pure/xmtp_sdk");
type NativeOnlyExports =
  | "LogProcessType"
  | "LogRotation"
  | "NotificationChannel"
  | "NotificationChannel_Tags"
  | "NotificationConfig"
  | "NotificationFailure"
  | "NotificationState"
  | "NotificationState_Tags"
  | "decryptFile"
  | "encryptFile"
  | "enterDebugWriter"
  | "exitDebugWriter"
  | "setLogSinkQueued";
// Pure WASM also exports these host classes from its raw module. Node exports
// them from index.ts instead.
type PureHostExports =
  | "ConversationID"
  | "InboxID"
  | "InstallationID"
  | "Message"
  | "MessageID"
  | "Timestamp";
export type ExportParity = Assert<
  Equal<
    Exclude<NativeExports, NativeOnlyExports>,
    Exclude<BrowserExports, PureHostExports>
  >
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
  SameMethods<Node.StorageLike, Browser.StorageLike, "delete_">
>;
export type SignatureRequestParity = Assert<
  SameMethods<Node.SignatureRequestLike, Browser.SignatureRequestLike>
>;
export type ClientParity = Assert<
  SameMethods<
    Node.ClientLike,
    Browser.ClientLike,
    | "catchUpToLive"
    | "disableNotifications"
    | "enableNotifications"
    | "notificationState"
  >
>;

// Check records passed across the bridge as well as object method names.
export type ClientOptionsParity = Assert<
  SameFields<Node.ClientOptions, Browser.ClientOptions>
>;
export type EncodedContentParity = Assert<
  SameFields<Node.EncodedContent, Browser.EncodedContent>
>;
export type MessageDataParity = Assert<
  SameFields<Node.MessageData, Browser.MessageData>
>;
export type StorageOptionsParity = Assert<
  SameFields<Node.StorageOptions, Browser.StorageOptions>
>;
