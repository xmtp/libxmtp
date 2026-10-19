// Rules for the interactions on one message bubble: which context-menu items
// show, when a swipe arms a reply, which new row a send flight lands on, and
// what the Reactions sheet lists.

import {
  type ChatMessage,
  type ReactionBadgeModel,
  contentText,
  isDeleted,
} from "./chatMessages.ts";

/** The quick reactions above a message in its context menu, in order. */
export const QUICK_REACTIONS = ["👍", "❤️", "😂", "😮", "😢"] as const;

export const MessageMenuCopy = {
  copy: "Copy",
  reply: "Reply",
  react: "React",
  delete: "Delete",
  retry: "Retry",
  edit: "Edit",
  more: "More",
  deleteTitle: "Delete message?",
  deleteMessage:
    "This sends a deletion request to this conversation. Other members may already have a copy.",
  deleteConfirm: "Delete message",
} as const;

export type MessageMenuItem =
  | "copy"
  | "reply"
  | "react"
  | "delete"
  | "retry"
  | "edit";

/**
 * The context-menu items for `row`: Copy always; Reply and React when allowed
 * and not deleted; then, for own rows when allowed, Delete once published,
 * else Retry and Edit. Delete also shows on an own deleted row, where tapping
 * it does nothing (`requestDelete` rejects deleted rows, so no confirmation
 * appears).
 */
export function menuItems(
  row: ChatMessage,
  own: string,
  allowed: boolean,
): MessageMenuItem[] {
  const items: MessageMenuItem[] = ["copy"];
  if (allowed && !isDeleted(row.content)) items.push("reply", "react");
  if (allowed && row.sender === own) {
    if (row.delivery === "published") items.push("delete");
    else items.push("retry", "edit");
  }
  return items;
}

/**
 * The VoiceOver actions: Reply and React when allowed and not deleted, Copy,
 * and Delete for own published rows.
 */
export function accessibilityItems(
  row: ChatMessage,
  own: string,
  allowed: boolean,
): MessageMenuItem[] {
  const items: MessageMenuItem[] = [];
  if (allowed && !isDeleted(row.content)) items.push("reply", "react");
  items.push("copy");
  if (allowed && row.sender === own && row.delivery === "published") {
    items.push("delete");
  }
  return items;
}

/** A swipe arms at 48 pt. */
export const SWIPE_REPLY_DISTANCE = 48;
/** The swipe pan starts after 12 pt sideways and fails after 12 pt vertically. */
export const SWIPE_MINIMUM_DISTANCE = 12;

/**
 * Swipe to reply arms when the row can be replied to and the drag moved at
 * least 48 pt sideways, more than it moved vertically, in either direction.
 */
export function swipeArmed(
  translationX: number,
  translationY: number,
  canReply: boolean,
): boolean {
  const dx = Math.abs(translationX);
  return canReply && dx >= SWIPE_REPLY_DISTANCE && dx > Math.abs(translationY);
}

/** Whether a reply can start from `row` (swipe): allowed and not deleted. */
export function canReply(row: ChatMessage, allowed: boolean): boolean {
  return allowed && !isDeleted(row.content);
}

/**
 * The row a send flight lands on when the newest row changes: the newest row,
 * when it is own and shows the submitted text.
 */
export function flightTarget(
  messages: readonly ChatMessage[],
  own: string,
  submitted: string | undefined,
): ChatMessage | undefined {
  if (submitted === undefined) return undefined;
  const last = messages.at(-1);
  if (last === undefined || last.sender !== own) return undefined;
  return contentText(last.content) === submitted ? last : undefined;
}

/** The flying bubble is as wide as its target, at most 300 pt. */
export const FLIGHT_MAX_WIDTH = 300;
/** The flight starts at 92 % of its size. */
export const FLIGHT_START_SCALE = 0.92;

/** One row of the Reactions sheet list: a sender and the emoji they chose. */
export interface ReactionListRow {
  readonly sender: string;
  readonly emoji: string;
}

/** "All N": the number of reactions over every badge. */
export function reactionTotal(badges: readonly ReactionBadgeModel[]): number {
  return badges.reduce((sum, badge) => sum + badge.count, 0);
}

/**
 * The list rows for a filter: every badge when `selected` is "" (All),
 * else the badge with that emoji; senders in badge order.
 */
export function reactionRows(
  badges: readonly ReactionBadgeModel[],
  selected: string,
): ReactionListRow[] {
  return badges
    .filter((badge) => selected === "" || badge.emoji === selected)
    .flatMap((badge) =>
      badge.senders.map((sender) => ({ sender, emoji: badge.emoji })),
    );
}

/**
 * The emoji "Remove my reaction" removes: the first own badge in the filter,
 * when the conversation is allowed.
 */
export function removableReaction(
  badges: readonly ReactionBadgeModel[],
  selected: string,
  allowed: boolean,
): string | undefined {
  if (!allowed) return undefined;
  return badges.find(
    (badge) => badge.mine && (selected === "" || badge.emoji === selected),
  )?.emoji;
}
