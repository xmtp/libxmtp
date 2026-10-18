// Conversation titles and identity labels from the iOS example
// (ConversationModels.swift, IdentityDirectory.swift, DetailsModels.swift).

import { groupInitials, personInitials } from "./avatar.ts";

export type ConversationKind = "group" | "dm";

/** The conversation fields that its title depends on. */
export interface TitleSource {
  readonly id: string;
  readonly kind: ConversationKind;
  /** The group name. Ignored for a DM. */
  readonly name?: string;
  readonly peerInboxId?: string;
  readonly peerHandle?: string;
}

/** Characters of an inbox ID shown where no handle is known. */
export const SHORT_INBOX_ID_LENGTH = 12;
/** Characters of an inbox ID shown for a message sender without a handle. */
export const SENDER_INBOX_ID_LENGTH = 8;

/** The first 12 characters of an inbox ID (8 for a message sender). */
export function shortInboxId(inboxId: string, sender = false): string {
  return inboxId.slice(
    0,
    sender ? SENDER_INBOX_ID_LENGTH : SHORT_INBOX_ID_LENGTH,
  );
}

const hasText = (value: string | undefined): value is string =>
  value !== undefined && value.trim().length > 0;

/**
 * The conversation title: a non-blank local alias; a group's name or
 * "Group"; a DM's `@handle`, its peer's short inbox ID, or "Direct message".
 */
export function conversationTitle(
  conversation: TitleSource,
  alias?: string,
): string {
  if (hasText(alias)) return alias;
  if (conversation.kind === "group") {
    const name = conversation.name ?? "";
    return name.trim().length === 0 ? "Group" : name;
  }
  if (conversation.peerHandle !== undefined && conversation.peerHandle !== "") {
    return "@" + conversation.peerHandle;
  }
  if (
    conversation.peerInboxId !== undefined &&
    conversation.peerInboxId !== ""
  ) {
    return shortInboxId(conversation.peerInboxId);
  }
  return "Direct message";
}

/**
 * Avatar initials for a conversation row with `title`. A DM whose title is
 * its peer's short inbox ID uses the inbox ID rule (`ConversationRow.initials`).
 */
export function conversationInitials(
  conversation: TitleSource,
  title: string,
): string {
  if (conversation.kind === "group") return groupInitials(title);
  const isInboxId =
    conversation.peerInboxId !== undefined &&
    title === shortInboxId(conversation.peerInboxId);
  return personInitials(title, isInboxId);
}

/**
 * The Details header avatar label (`ConversationDetails.avatarLabel`): a DM
 * uses the inbox ID rule when it has no alias and no handle.
 */
export function detailsAvatarLabel(
  conversation: TitleSource,
  alias?: string,
): string {
  const title = conversationTitle(conversation, alias);
  if (conversation.kind === "group") return groupInitials(title);
  const isInboxId =
    !hasText(alias) &&
    (conversation.peerHandle === undefined || conversation.peerHandle === "") &&
    conversation.peerInboxId !== undefined;
  return personInitials(title, isInboxId);
}

/**
 * The key that read markers (and iOS mutes) use: a group's ID, or
 * `dm:<own>:<peer>` with the two inbox IDs sorted, so duplicate DMs with one
 * peer share it.
 */
export function logicalKey(
  conversation: TitleSource,
  ownInboxId: string,
): string {
  if (conversation.kind === "group") return conversation.id;
  return "dm:" + [ownInboxId, conversation.peerInboxId ?? ""].sort().join(":");
}

/** `@handle`, else the short inbox ID (`IdentityDirectory.displayName`). */
export function displayName(
  inboxId: string,
  handle: string | undefined,
  sender = false,
): string {
  return handle === undefined ? shortInboxId(inboxId, sender) : "@" + handle;
}

/** A member row title: the handle without `@`, else the short inbox ID. */
export function memberTitle(inboxId: string, handle?: string): string {
  return handle ?? shortInboxId(inboxId);
}

/** A member avatar label: the first character of the member title, uppercased. */
export function memberInitial(inboxId: string, handle?: string): string {
  return Array.from(memberTitle(inboxId, handle))
    .slice(0, 1)
    .join("")
    .toUpperCase();
}

/** The handle without one leading `@` (`HandleText.normalized`). */
export function stripAt(handle: string): string {
  return handle.startsWith("@") ? handle.slice(1) : handle;
}

/**
 * A handle split once at the first `#` for display: the name, then the
 * `#code` suffix (empty when there is none). One leading `@` is dropped first.
 */
export function splitHandle(handle: string): {
  readonly name: string;
  readonly code: string;
} {
  const value = stripAt(handle);
  const index = value.indexOf("#");
  return index < 0
    ? { name: value, code: "" }
    : { name: value.slice(0, index), code: value.slice(index) };
}

/**
 * Whether a conversation row matches a search: its title, peer handle,
 * preview or ID contains `search`, ignoring case.
 */
export function rowMatches(
  fields: {
    readonly title: string;
    readonly peerHandle?: string;
    readonly preview: string;
    readonly id: string;
  },
  search: string,
): boolean {
  const needle = search.toLocaleLowerCase();
  return [
    fields.title,
    fields.peerHandle ?? "",
    fields.preview,
    fields.id,
  ].some((value) => value.toLocaleLowerCase().includes(needle));
}
