// Test support for the chat stores: an in-memory conversation that answers
// `ChatQueries` like the SDK (published history pages of 50 with exclusive
// bounds, pending pages, optimistic queue and publish). Only tests import it.

import type {
  ChatPage,
  ChatQueries,
  OutboundContent,
  PendingPage,
} from "./chatHistory.ts";
import type {
  ChatMessage,
  ConsentState,
  ConversationRecord,
  HistoryPosition,
  RecoveryPosition,
} from "./chatMessages.ts";
import {
  type SendDraft,
  type SendJournalStorage,
  SendError,
} from "./sendJournal.ts";

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
  historyCalls = 0;
  queued: OutboundContent[] = [];
  publications: string[] = [];
  deletions: string[] = [];
  /** Runs inside `queue`, before it returns (journal checks). */
  onQueue: (() => void) | undefined;
  queueError: Error | undefined;
  publishError: Error | undefined;
  historyError: Error | undefined;
  /** Holds history reads until released. */
  gate: Promise<void> | undefined;
  #next = 10_000;
  readonly #deletionTargets = new Map<string, string>();

  constructor(count = 0) {
    for (let n = 1; n <= count; n += 1) this.published.push(textMessage(n));
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
    this.historyCalls += 1;
    if (this.gate !== undefined) await this.gate;
    if (this.historyError !== undefined) throw this.historyError;
    const inRange = this.published.filter((row) => {
      const position = row.position;
      if (position === undefined) return false;
      if (before !== undefined && compare(position, before) >= 0) return false;
      if (after !== undefined && compare(position, after) <= 0) return false;
      return true;
    });
    const ordered = ascending ? inRange : [...inRange].reverse();
    const rows = ordered.slice(0, 50);
    return {
      rows,
      ...(rows[0]?.position === undefined ? {} : { first: rows[0].position }),
      ...(rows.at(-1)?.position === undefined
        ? {}
        : { last: rows.at(-1)?.position }),
      hasMore: ordered.length > 50,
      skipped: 0,
    };
  }

  pending(_id: string, before?: RecoveryPosition): Promise<PendingPage> {
    const ordered = [...this.unpublished]
      .reverse()
      .filter((row) => before === undefined || row.sentAtNs < before.sentAtNs);
    const rows = ordered.slice(0, 50);
    const last = rows.at(-1);
    return Promise.resolve({
      rows,
      ...(last === undefined
        ? {}
        : { last: { sentAtNs: last.sentAtNs, cursor: last.id } }),
      hasMore: ordered.length > 50,
      skipped: 0,
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

/** Resolves after pending promise callbacks ran. */
export async function settle(): Promise<void> {
  for (let index = 0; index < 10; index += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}
