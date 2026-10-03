import type {
  Client,
  Conversation,
  Dm,
  Group,
  NotificationOverride,
} from "@xmtp/browser-sdk";

type Assert<T extends true> = T;
type NativePushMethods =
  | "enableNotifications"
  | "disableNotifications"
  | "notificationState";
type HasNoNativePush<T> =
  Extract<keyof T, NativePushMethods> extends never ? true : false;

export type ClientHasNoNativePush = Assert<HasNoNativePush<Client>>;
export type ConversationHasNoNativePush = Assert<HasNoNativePush<Conversation>>;
export type GroupHasNoNativePush = Assert<HasNoNativePush<Group>>;
export type DmHasNoNativePush = Assert<HasNoNativePush<Dm>>;
// Conversation notification metadata stays public on all targets.
export type MetadataOverride = Assert<
  NotificationOverride extends "enabled" | "disabled" | "default" ? true : false
>;
export type GroupMetadataSetter = Assert<
  Group["setNotifications"] extends (
    value: NotificationOverride,
  ) => Promise<void>
    ? true
    : false
>;

// @ts-expect-error Browser has no native notification configuration type.
export type { NotificationConfig } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no native notification channel type.
export type { NotificationChannel } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no native notification state type.
export type { NotificationState } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no native notification error type.
export type { NotificationError } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no native notification failure type.
export type { NotificationFailure } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no handwritten worker client.
export type { WorkerClient } from "@xmtp/browser-sdk";
// @ts-expect-error Browser has no handwritten OPFS dispatcher.
export type { Opfs } from "@xmtp/browser-sdk";
