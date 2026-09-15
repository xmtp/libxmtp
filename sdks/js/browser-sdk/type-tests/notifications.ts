import type { Client, Conversation, Dm, Group } from "@xmtp/browser-sdk";
import type {
  Client as WasmClient,
  Conversation as WasmConversation,
} from "@xmtp/wasm-bindings";

type Assert<T extends true> = T;
type PushMethods =
  | "enableNotifications"
  | "disableNotifications"
  | "notificationState"
  | "setNotifications"
  | "notificationsEnabled";
type HasNoPush<T> = Extract<keyof T, PushMethods> extends never ? true : false;

export type ClientHasNoPush = Assert<HasNoPush<Client>>;
export type ConversationHasNoPush = Assert<HasNoPush<Conversation>>;
export type GroupHasNoPush = Assert<HasNoPush<Group>>;
export type DmHasNoPush = Assert<HasNoPush<Dm>>;
export type WasmClientHasNoPush = Assert<HasNoPush<WasmClient>>;
export type WasmConversationHasNoPush = Assert<HasNoPush<WasmConversation>>;

// @ts-expect-error Browser has no notification configuration type.
export type { NotificationConfig } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no notification channel type.
export type { NotificationChannel } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no notification state type.
export type { NotificationState } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no notification override type.
export type { NotificationOverride } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no notification error type.
export type { NotificationError } from "@xmtp/browser-sdk";
// @ts-expect-error WASM has no notification configuration type.
export type { NotificationConfig as WasmNotificationConfig } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no notification channel type.
export type { NotificationChannel as WasmNotificationChannel } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no notification state type.
export type { NotificationState as WasmNotificationState } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no notification override type.
export type { NotificationOverride as WasmNotificationOverride } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no notification error type.
export type { NotificationError as WasmNotificationError } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no native notification failure enum.
export type { NotificationFailure as WasmNotificationFailure } from "@xmtp/wasm-bindings";
// @ts-expect-error WASM has no native notification state enum.
export type { NotificationStateKind as WasmNotificationStateKind } from "@xmtp/wasm-bindings";
