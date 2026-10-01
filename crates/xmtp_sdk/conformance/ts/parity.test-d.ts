// Compare public method parameters and returns, and record fields, of the
// Node and browser package roots structurally (plan P62).
// check-parity-signatures.py compares every root declaration as text. SDK-037
// is the only removal list.
import type * as Node from "../../../../target/sdk-generated/typescript-napi/index";
import type * as Browser from "../../../../target/sdk-generated/typescript-wasm/index";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <
    Value,
  >() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Value extends true> = Value;
// Public objects and classes become their name, which ends the recursion;
// each one is compared where it is declared.
type Opaque<Value> = Value extends Node.Message | Browser.Message
  ? "Message"
  : Value extends Node.Client | Browser.Client
    ? "Client"
    : Value extends Node.Group | Browser.Group
      ? "Group"
      : Value extends Node.Dm | Browser.Dm
        ? "Dm"
        : Value extends Node.Conversations | Browser.Conversations
          ? "Conversations"
          : Value extends Node.MessageReader | Browser.MessageReader
            ? "MessageReader"
            : Value extends Node.ConversationReader | Browser.ConversationReader
              ? "ConversationReader"
              : Value extends Node.SignatureRequest | Browser.SignatureRequest
                ? "SignatureRequest"
                : Value extends Node.Backend | Browser.Backend
                  ? "Backend"
                  : Value extends Node.Storage | Browser.Storage
                    ? "Storage"
                    : Value extends Node.Archives | Browser.Archives
                      ? "Archives"
                      : Value extends Node.Preferences | Browser.Preferences
                        ? "Preferences"
                        : Value extends Node.Diagnostics | Browser.Diagnostics
                          ? "Diagnostics"
                          : Value extends Node.Timestamp | Browser.Timestamp
                            ? "Timestamp"
                            : Value extends Node.XmtpError | Browser.XmtpError
                              ? "XmtpError"
                              : Value extends Uint8Array
                                ? "Uint8Array"
                                : never;
// Canonical keeps every literal, union member, optional marker, and nested
// field. It maps each member of a union on its own, so `undefined` and other
// members stay beside an opaque type.
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
// Symbol keys are module-private brands; each target has its own.
type Keys<Value> = Exclude<keyof Value, symbol>;
type MismatchKeys<Native, Web, Removed extends PropertyKey = never> = {
  [Key in Exclude<Keys<Native>, Removed> & Keys<Web>]: Equal<
    Canonical<Native[Key]>,
    Canonical<Web[Key]>
  > extends true
    ? never
    : Key;
}[Exclude<Keys<Native>, Removed> & Keys<Web>];
type SameMethods<Native, Web, Removed extends PropertyKey = never> =
  Equal<Exclude<Keys<Native>, Removed>, Keys<Web>> extends true
    ? [MismatchKeys<Native, Web, Removed>] extends [never]
      ? true
      : false
    : false;
// Expand the record itself: Opaque would reduce a whole record to its name.
type SameFields<Native, Web> =
  Equal<keyof Native, keyof Web> extends true
    ? Equal<Structure<Native>, Structure<Web>>
    : false;

// check-parity-signatures.py compares every export of both roots, value and
// type-only, and holds the SDK-037 list. This type test compares object
// methods and record shapes structurally.
type PublicNodeExports =
  keyof typeof import("../../../../target/sdk-generated/typescript-napi/index");
export type QueuedLogSinkIsInternal = Assert<
  Equal<"setLogSinkQueued" extends PublicNodeExports ? true : false, false>
>;

// verifies: LOG-002, LOG-007
export type NodeLogSinkIsAsync = Assert<
  Equal<ReturnType<Node.LogSink["log"]>, Promise<void>>
>;
export type BrowserLogSinkIsAsync = Assert<
  Equal<ReturnType<Browser.LogSink["log"]>, Promise<void>>
>;
export type LogAdmissionIsPrivate = Assert<
  Equal<"sdkLogSinkHandoff" extends PublicNodeExports ? true : false, false>
>;

export type BackendParity = Assert<SameMethods<Node.Backend, Browser.Backend>>;
export type MessageReaderParity = Assert<
  SameMethods<Node.MessageReader, Browser.MessageReader>
>;
export type ConversationReaderParity = Assert<
  SameMethods<Node.ConversationReader, Browser.ConversationReader>
>;
export type GroupParity = Assert<SameMethods<Node.Group, Browser.Group>>;
export type DmParity = Assert<SameMethods<Node.Dm, Browser.Dm>>;
export type ArchivesParity = Assert<
  SameMethods<
    Node.Archives,
    Browser.Archives,
    "exportToFile" | "importFromFile" | "metadataFromFile"
  >
>;
export type ConversationsParity = Assert<
  SameMethods<Node.Conversations, Browser.Conversations>
>;
export type DiagnosticsParity = Assert<
  SameMethods<Node.Diagnostics, Browser.Diagnostics>
>;
export type PreferencesParity = Assert<
  SameMethods<Node.Preferences, Browser.Preferences>
>;
export type StorageParity = Assert<
  SameMethods<Node.Storage, Browser.Storage, "delete_" | "reconnect">
>;
export type BrowserStorageOmitsReconnect = Assert<
  Equal<"reconnect" extends keyof Browser.Storage ? true : false, false>
>;
export type SignatureRequestParity = Assert<
  SameMethods<Node.SignatureRequest, Browser.SignatureRequest>
>;
// `options` differs only by the SDK-037 storage key; ClientOptionsParity
// compares it.
export type ClientParity = Assert<
  SameMethods<
    Omit<Node.Client, "options">,
    Omit<Browser.Client, "options">,
    "disableNotifications" | "enableNotifications" | "notificationState"
  >
>;
export type MessageParity = Assert<SameMethods<Node.Message, Browser.Message>>;

// Records passed across the bridge.
export type ClientOptionsParity = Assert<
  SameFields<
    Omit<Node.ClientOptions, "storage"> & {
      readonly storage: Omit<Node.StorageOptions, "encryptionKey">;
    },
    Browser.ClientOptions
  >
>;
export type EncodedContentParity = Assert<
  SameFields<Node.EncodedContent, Browser.EncodedContent>
>;
export type StorageOptionsParity = Assert<
  SameFields<Omit<Node.StorageOptions, "encryptionKey">, Browser.StorageOptions>
>;

// The browser storage admin opens without a Client. Its bytes are Uint8Array.
export type AdminFactoryArguments = Assert<
  Equal<Parameters<typeof Browser.Storage.admin>, []>
>;
export type AdminFactoryResult = Assert<
  Equal<ReturnType<typeof Browser.Storage.admin>, Promise<Browser.StorageAdmin>>
>;
export type AdminExportBytes = Assert<
  Equal<ReturnType<Browser.StorageAdmin["exportDb"]>, Promise<Uint8Array>>
>;
export type AdminImportBytes = Assert<
  Equal<Parameters<Browser.StorageAdmin["importDb"]>, [string, Uint8Array]>
>;
export type AdminMethods = Assert<
  Equal<
    keyof Browser.StorageAdmin,
    | "listFiles"
    | "fileCount"
    | "poolCapacity"
    | "fileExists"
    | "deleteFile"
    | "exportDb"
    | "importDb"
    | "clearAll"
    | "end"
  >
>;
