// Test support for the chat stores: an in-memory conversation that answers
// `ChatQueries` like the SDK (published history pages of 50 with exclusive
// bounds, pending pages, optimistic queue and publish). Only tests import it.

import type {
  ChatPage,
  ChatQueries,
  OutboundContent,
  PendingPage,
  ReadingAnchor,
} from "./chatHistory.ts";
import type {
  ChatMessage,
  ConsentState,
  ConversationRecord,
  HistoryPosition,
  ReactionRecord,
  RecoveryPosition,
} from "./chatMessages.ts";
import {
  SendCoordinator,
  type SendDraft,
  SendError,
  type SendJournal,
  type SendJournalStorage,
} from "./sendJournal.ts";
import { deferred } from "./testing.ts";

export const CONVERSATION = "chat";
export const OWN = "own-inbox";
export const PEER = "peer-inbox";

/** A published text message `n` (sent at n seconds). */
export function textMessage(
  n: number,
  options: { sender?: string; text?: string; conversationId?: string } = {},
): ChatMessage {
  const sentAtNs = BigInt(n) * 1_000_000_000n;
  const id = `m${String(n).padStart(4, "0")}`;
  return {
    id,
    conversationId: options.conversationId ?? CONVERSATION,
    sender: options.sender ?? PEER,
    sentAtNs,
    insertedAtNs: sentAtNs + 1n,
    delivery: "published",
    content: { kind: "text", text: options.text ?? `message ${String(n)}` },
    position: { sentAtNs, cursor: id },
    reactions: [],
  };
}

/** Message `n` as its sender deleted it. */
export function deletedMessage(n: number): ChatMessage {
  return { ...textMessage(n), content: { kind: "deleted" } };
}

/** The reading anchor at message `n`. */
export function anchorOf(n: number): ReadingAnchor {
  const { id, sentAtNs } = textMessage(n);
  return { messageId: id, position: { sentAtNs, cursor: id } };
}

/** A reaction record by `sender`, sent at `sentAt` nanoseconds. */
export function reactionRecord(
  sender: string,
  emoji: string,
  sentAt: number,
  options: { added?: boolean; schema?: string } = {},
): ReactionRecord {
  return {
    sender,
    emoji,
    schema: options.schema ?? "unicode",
    added: options.added ?? true,
    sentAtNs: BigInt(sentAt),
  };
}

function compare(left: HistoryPosition, right: HistoryPosition): number {
  if (left.sentAtNs !== right.sentAtNs) {
    return left.sentAtNs < right.sentAtNs ? -1 : 1;
  }
  return left.cursor < right.cursor ? -1 : left.cursor > right.cursor ? 1 : 0;
}

/** An in-memory conversation behind `ChatQueries`. */
export class FakeConversation implements ChatQueries {
  consent: ConsentState = "allowed";
  kind: "group" | "dm" = "dm";
  /** Published messages, oldest first. */
  published: ChatMessage[] = [];
  /** Unpublished or failed messages, oldest first. */
  unpublished: ChatMessage[] = [];
  queued: OutboundContent[] = [];
  publications: string[] = [];
  deletions: string[] = [];
  /** Runs inside `queue`, before it returns (journal checks). */
  onQueue: (() => void) | undefined;
  /** Runs inside `delete`, before it returns (journal checks). */
  onDelete: ((messageId: string) => void) | undefined;
  queueError: Error | undefined;
  publishError: Error | undefined;
  historyError: Error | undefined;
  /**
   * IDs of stored messages that cannot be decoded. Pages leave them out and
   * count them in `skipped`; the page positions still cover them, like the
   * SDK's `skippedCount`.
   */
  readonly unreadable = new Set<string>();
  #next = 10_000;
  #hold: Promise<void> | undefined;
  readonly #deletionTargets = new Map<string, string>();

  constructor(count = 0) {
    for (let n = 1; n <= count; n += 1) this.published.push(textMessage(n));
  }

  /** Holds the next history read until the returned function runs. */
  hold(): () => void {
    const gate = deferred();
    this.#hold = gate.promise;
    return () => gate.resolve();
  }

  conversation(id: string): Promise<ConversationRecord> {
    if (id !== CONVERSATION) {
      return Promise.reject(new SendError("unavailable"));
    }
    return Promise.resolve({
      id,
      kind: this.kind,
      consent: this.consent,
      ...(this.kind === "dm" ? { peerInboxId: PEER } : { name: "Group" }),
    });
  }

  async history(
    _id: string,
    ascending: boolean,
    before?: HistoryPosition,
    after?: HistoryPosition,
  ): Promise<ChatPage> {
    const held = this.#hold;
    this.#hold = undefined;
    if (held !== undefined) await held;
    if (this.historyError !== undefined) throw this.historyError;
    const inRange = this.published.filter((row) => {
      const position = row.position;
      if (position === undefined) return false;
      if (before !== undefined && compare(position, before) >= 0) return false;
      if (after !== undefined && compare(position, after) <= 0) return false;
      return true;
    });
    const ordered = ascending ? inRange : [...inRange].reverse();
    const page = ordered.slice(0, 50);
    const rows = page.filter((row) => !this.unreadable.has(row.id));
    const first = page[0]?.position;
    const last = page.at(-1)?.position;
    return {
      rows,
      ...(first === undefined ? {} : { first }),
      ...(last === undefined ? {} : { last }),
      hasMore: ordered.length > 50,
      skipped: page.length - rows.length,
    };
  }

  pending(_id: string, before?: RecoveryPosition): Promise<PendingPage> {
    const ordered = [...this.unpublished]
      .reverse()
      .filter((row) => before === undefined || row.sentAtNs < before.sentAtNs);
    const page = ordered.slice(0, 50);
    const rows = page.filter((row) => !this.unreadable.has(row.id));
    const last = page.at(-1);
    return Promise.resolve({
      rows,
      ...(last === undefined
        ? {}
        : { last: { sentAtNs: last.sentAtNs, cursor: last.id } }),
      hasMore: ordered.length > 50,
      skipped: page.length - rows.length,
    });
  }

  message(id: string): Promise<ChatMessage | undefined> {
    return Promise.resolve(
      [...this.published, ...this.unpublished].find((row) => row.id === id),
    );
  }

  queue(conversationId: string, content: OutboundContent): Promise<string> {
    this.onQueue?.();
    if (this.queueError !== undefined) {
      const error = this.queueError;
      this.queueError = undefined;
      return Promise.reject(error);
    }
    this.queued.push(content);
    this.#next += 1;
    const sentAtNs = BigInt(this.#next) * 1_000_000_000n;
    const id = `q${String(this.#next)}`;
    if (content.kind === "text" || content.kind === "reply") {
      this.unpublished.push({
        id,
        conversationId,
        sender: OWN,
        sentAtNs,
        insertedAtNs: sentAtNs,
        delivery: "queued",
        content:
          content.kind === "text"
            ? { kind: "text", text: content.text }
            : { kind: "reply", quote: "Reply", body: content.text },
        reactions: [],
      });
    }
    return Promise.resolve(id);
  }

  publish(_conversationId: string, messageId: string): Promise<void> {
    this.publications.push(messageId);
    if (this.publishError !== undefined) {
      return Promise.reject(this.publishError);
    }
    const index = this.unpublished.findIndex((row) => row.id === messageId);
    const target = this.#deletionTargets.get(messageId);
    if (target !== undefined) this.redact(target);
    for (const row of index < 0 ? [] : this.unpublished.splice(index, 1)) {
      this.published.push({
        ...row,
        delivery: "published",
        position: { sentAtNs: row.sentAtNs, cursor: row.id },
      });
    }
    return Promise.resolve();
  }

  delete(messageId: string): Promise<string> {
    this.onDelete?.(messageId);
    this.deletions.push(messageId);
    this.#next += 1;
    const id = `d${String(this.#next)}`;
    const sentAtNs = BigInt(this.#next) * 1_000_000_000n;
    this.#deletionTargets.set(id, messageId);
    this.unpublished.push({
      id,
      conversationId: CONVERSATION,
      sender: OWN,
      sentAtNs,
      insertedAtNs: sentAtNs,
      delivery: "queued",
      content: { kind: "reaction" },
      reactions: [],
    });
    return Promise.resolve(id);
  }

  /** Marks a published message deleted (its sender deleted it). */
  redact(messageId: string): void {
    this.published = this.published.map((row) =>
      row.id === messageId
        ? { ...row, content: { kind: "deleted" }, reactions: [] }
        : row,
    );
  }
}

/** A send coordinator over `queries`, mapped like the SDK adapter. */
export function coordinatorFor(
  queries: FakeConversation,
  journal: SendJournal,
): SendCoordinator {
  return new SendCoordinator(
    journal,
    {
      consent: async (id) => (await queries.conversation(id)).consent,
      message: async (id) => {
        const row = await queries.message(id);
        return row === undefined
          ? undefined
          : {
              conversationId: row.conversationId,
              published: row.delivery === "published",
            };
      },
      publish: (conversationId, messageId) =>
        queries.publish(conversationId, messageId),
    },
    () => true,
  );
}

/** In-memory `send-drafts`. */
export class MemoryJournal implements SendJournalStorage {
  drafts: readonly SendDraft[] = [];

  read(): readonly SendDraft[] {
    return this.drafts;
  }

  write(drafts: readonly SendDraft[]): void {
    this.drafts = drafts;
  }
}
