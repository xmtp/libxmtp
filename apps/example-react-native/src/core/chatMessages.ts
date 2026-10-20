// Chat message values and the rules that map them for display.
//
// The SDK adapter (`features/chat/sdkChatQueries.ts`) builds these values; the
// timeline, composer and action stores and every view read only these.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import type { ConversationKind } from "./titles.ts";

/** Copy the mapping emits. */
export const ChatContentCopy = {
  deleted: "Message deleted",
  file: "File",
  replyFallback: "Reply",
  unsupportedContent: "Unsupported content",
  unsupportedReply: "Unsupported reply",
} as const;

/** `published`, `failed`, or anything else (`unpublished`): `queued`. */
export type MessageDelivery = "queued" | "published" | "failed";

/** A published message's history position (sent time, then delivery cursor). */
export interface HistoryPosition {
  readonly sentAtNs: bigint;
  readonly cursor: string;
}

/** An unpublished or failed message's recovery position. The cursor is opaque. */
export interface RecoveryPosition {
  readonly sentAtNs: bigint;
  readonly cursor: string;
}

/**
 * What a message row shows. A new content kind adds a case here, a case in
 * the SDK mapping (`sdkChatQueries.ts` `chatContent`), a case in
 * `contentText`, and a renderer in `features/chat/MessageContent.tsx`.
 */
export type ChatContent =
  | { readonly kind: "text"; readonly text: string }
  /** Shown as plain text (no markdown rendering). */
  | { readonly kind: "markdown"; readonly text: string }
  | {
      readonly kind: "reply";
      /** The quoted parent text ("Reply" when unknown, "Message deleted"). */
      readonly quote: string;
      readonly body: string;
      readonly parentId?: string;
    }
  | { readonly kind: "attachment"; readonly file: RemoteAttachment }
  | { readonly kind: "deleted" }
  | { readonly kind: "unsupported"; readonly text: string }
  /** Never rendered: reactions fold into badges on their parent. */
  | { readonly kind: "reaction" };

export interface ReactionRecord {
  readonly sender: string;
  readonly emoji: string;
  readonly schema: string;
  /** False for a removal. */
  readonly added: boolean;
  readonly sentAtNs: bigint;
}

export interface ChatMessage {
  readonly id: string;
  readonly conversationId: string;
  /** The sender's inbox ID. */
  readonly sender: string;
  readonly sentAtNs: bigint;
  readonly insertedAtNs: bigint;
  readonly delivery: MessageDelivery;
  readonly content: ChatContent;
  /** Present for published messages only. */
  readonly position?: HistoryPosition;
  readonly reactions: readonly ReactionRecord[];
}

/** One reaction badge under a bubble. */
export interface ReactionBadgeModel {
  readonly emoji: string;
  /** Inbox IDs, sorted. */
  readonly senders: readonly string[];
  /** The own inbox reacted with this emoji. */
  readonly mine: boolean;
  readonly count: number;
}

export type ConsentState = "allowed" | "unknown" | "denied";

/** The conversation fields the chat screen shows (`ConversationRecord`). */
export interface ConversationRecord {
  readonly id: string;
  readonly kind: ConversationKind;
  /** The group name. */
  readonly name?: string;
  readonly peerInboxId?: string;
  readonly peerHandle?: string;
  readonly consent: ConsentState;
  /** Groups only; undefined when the member read failed. */
  readonly memberCount?: number;
}

/** The text a row's content stands for: copy, reply quotes, previews. */
export function contentText(content: ChatContent): string {
  switch (content.kind) {
    case "text":
    case "markdown":
    case "unsupported":
      return content.text;
    case "reply":
      return content.body;
    case "attachment":
      return content.file.filename ?? ChatContentCopy.file;
    case "deleted":
      return ChatContentCopy.deleted;
    case "reaction":
      return "";
  }
}

export function isDeleted(content: ChatContent): boolean {
  return content.kind === "deleted";
}

/** The message's local date (sent time). */
export function messageDate(message: ChatMessage): Date {
  return new Date(Number(message.sentAtNs / 1_000_000n));
}

/** Orders strings by Unicode scalar values, like Swift `String <` on emoji. */
function compareScalars(left: string, right: string): number {
  const a = Array.from(left);
  const b = Array.from(right);
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    const difference =
      (a[index]?.codePointAt(0) ?? 0) - (b[index]?.codePointAt(0) ?? 0);
    if (difference !== 0) return difference;
  }
  return a.length - b.length;
}

/**
 * Reaction badges: the latest record per sender, emoji and schema wins (ties
 * keep input order), removals drop out, badges group by emoji with sorted
 * senders and sort by emoji.
 */
export function reactionBadges(
  records: readonly ReactionRecord[],
  own: string,
): readonly ReactionBadgeModel[] {
  const ordered = records
    .map((record, index) => ({ record, index }))
    .sort((left, right) =>
      left.record.sentAtNs === right.record.sentAtNs
        ? left.index - right.index
        : left.record.sentAtNs < right.record.sentAtNs
          ? -1
          : 1,
    );
  const latest = new Map<string, ReactionRecord>();
  for (const { record } of ordered) {
    latest.set(
      JSON.stringify([record.sender, record.emoji, record.schema]),
      record,
    );
  }
  const groups = new Map<string, string[]>();
  for (const record of latest.values()) {
    if (!record.added) continue;
    const senders = groups.get(record.emoji) ?? [];
    senders.push(record.sender);
    groups.set(record.emoji, senders);
  }
  return Array.from(groups, ([emoji, senders]) => {
    const sorted = [...senders].sort(compareScalars);
    return {
      emoji,
      senders: sorted,
      mine: sorted.includes(own),
      count: sorted.length,
    };
  }).sort((left, right) => compareScalars(left.emoji, right.emoji));
}

/** True when the own inbox has an active reaction `emoji` on `message`. */
export function hasOwnReaction(
  message: ChatMessage,
  emoji: string,
  own: string,
): boolean {
  return reactionBadges(message.reactions, own).some(
    (badge) => badge.emoji === emoji && badge.mine,
  );
}
