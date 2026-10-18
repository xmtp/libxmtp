// Chat history windows, pages, the transcript cache and the reading anchor
// (ExampleCore/Chat/HistoryPages.swift, TranscriptCache.swift,
// AnchorRestore.swift, ChatTimelineStore.swift).
//
// Window rows are newest first, like the SDK's descending pages; the timeline
// reverses them for display.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import {
  type ChatMessage,
  type ConversationRecord,
  type HistoryPosition,
  type RecoveryPosition,
  isDeleted,
} from "./chatMessages.ts";

/** Copy the timeline emits (`ChatTimelineStore.swift`, `HistoryPages.swift`). */
export const TimelineCopy = {
  positionChanged: "Position changed",
  historySkipped: "Some stored history cannot be read. Load more to continue.",
  updatesStopped: "Message updates stopped.",
  readingPositionFailed: "Cannot save your reading position.",
  pendingSkippedMore:
    "Some stored pending messages cannot be read. Load more to continue.",
  pendingSkippedRefresh:
    "Some stored pending messages cannot be read. Refresh to try again.",
  noSurvivors: "No surviving messages are available.",
} as const;

/** SDK pages hold 50 messages. */
export const HISTORY_PAGE_SIZE = 50;
/** The transcript cache keeps this many rows per conversation… */
export const CACHE_ROW_LIMIT = 500;
/** …for this many conversations. */
export const CACHE_CONVERSATION_LIMIT = 3;

/** A loaded span of published history. */
export interface HistoryWindow {
  /** Newest first. */
  readonly rows: readonly ChatMessage[];
  /** The newest row's position (the start of a newer page). */
  readonly first?: HistoryPosition;
  /** The oldest row's position (the start of an older page). */
  readonly last?: HistoryPosition;
  readonly hasOlder: boolean;
  /** The window includes the newest page. */
  readonly atNewest: boolean;
  readonly notice?: string;
}

export const EMPTY_WINDOW: HistoryWindow = {
  rows: [],
  hasOlder: false,
  atNewest: true,
};

export interface ChatPage {
  /** In the requested direction. */
  readonly rows: readonly ChatMessage[];
  readonly first?: HistoryPosition;
  readonly last?: HistoryPosition;
  readonly hasMore: boolean;
  readonly skipped: number;
}

export interface PendingPage {
  /** Newest first. */
  readonly rows: readonly ChatMessage[];
  readonly last?: RecoveryPosition;
  readonly hasMore: boolean;
  readonly skipped: number;
}

/** What a message to send is (`OutboundContent`). */
export type OutboundContent =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "reply"; readonly parentId: string; readonly text: string }
  | {
      readonly kind: "reaction";
      readonly parentId: string;
      readonly emoji: string;
      readonly remove: boolean;
    }
  /** An uploaded file (`sendRemoteAttachment`). */
  | { readonly kind: "attachment"; readonly file: RemoteAttachment };

/**
 * The SDK reads and writes the chat stores use (`ChatQueries`). Errors that
 * prove nothing was queued are `SendError` rejected, unavailable or stale.
 */
export interface ChatQueries {
  conversation(id: string): Promise<ConversationRecord>;
  /** Published history, 50 per page; `before` and `after` are exclusive. */
  history(
    id: string,
    ascending: boolean,
    before?: HistoryPosition,
    after?: HistoryPosition,
  ): Promise<ChatPage>;
  /** Unpublished and failed messages, newest first. */
  pending(id: string, before?: RecoveryPosition): Promise<PendingPage>;
  message(id: string): Promise<ChatMessage | undefined>;
  /** Queues locally (optimistic) and returns the message ID. */
  queue(conversationId: string, content: OutboundContent): Promise<string>;
  publish(conversationId: string, messageId: string): Promise<void>;
  /** Queues a deletion of an own published message; returns its operation ID. */
  delete(messageId: string): Promise<string>;
}

/** Thrown when a read loop finds no position to continue from. */
export class HistoryUnavailableError extends Error {
  constructor() {
    super("History is unavailable.");
    this.name = "HistoryUnavailableError";
  }
}

/**
 * Reads pages until one has rows, has no more, or the budget runs out (pages
 * whose rows were all unreadable come back empty with `hasMore`).
 */
async function readable(
  queries: Pick<ChatQueries, "history">,
  id: string,
  ascending: boolean,
  bounds: { before?: HistoryPosition; after?: HistoryPosition },
  budget: number,
  checkpoint: () => void,
): Promise<{ readonly page: ChatPage; readonly count: number }> {
  let upper = bounds.before;
  let lower = bounds.after;
  let first: HistoryPosition | undefined;
  let skipped = 0;
  let count = 0;
  for (;;) {
    checkpoint();
    const page = await queries.history(id, ascending, upper, lower);
    checkpoint();
    count += 1;
    first ??= page.first;
    skipped += page.skipped;
    if (page.rows.length > 0 || !page.hasMore || count >= budget) {
      return {
        page: {
          rows: page.rows,
          ...(first === undefined ? {} : { first }),
          ...(page.last === undefined ? {} : { last: page.last }),
          hasMore: page.hasMore,
          skipped,
        },
        count,
      };
    }
    if (page.last === undefined) throw new HistoryUnavailableError();
    if (ascending) lower = page.last;
    else upper = page.last;
  }
}

/** The page of history before `before` (newest page when undefined). */
export async function olderWindow(
  queries: Pick<ChatQueries, "history">,
  id: string,
  before?: HistoryPosition,
  checkpoint: () => void = () => {},
): Promise<HistoryWindow> {
  const { page } = await readable(
    queries,
    id,
    false,
    before === undefined ? {} : { before },
    4,
    checkpoint,
  );
  return {
    rows: page.rows,
    ...(page.first === undefined ? {} : { first: page.first }),
    ...(page.last === undefined ? {} : { last: page.last }),
    hasOlder: page.hasMore,
    atNewest: before === undefined,
    ...(page.skipped > 0 ? { notice: TimelineCopy.historySkipped } : {}),
  };
}

/**
 * A window around `anchor`: up to two ascending pages after it, then the
 * descending pages before the first newer row.
 */
export async function aroundWindow(
  queries: Pick<ChatQueries, "history">,
  id: string,
  anchor: HistoryPosition,
  checkpoint: () => void = () => {},
): Promise<HistoryWindow> {
  const newer = await readable(
    queries,
    id,
    true,
    { after: anchor },
    2,
    checkpoint,
  );
  const older = await readable(
    queries,
    id,
    false,
    newer.page.first === undefined ? {} : { before: newer.page.first },
    4 - newer.count,
    checkpoint,
  );
  const first = newer.page.last ?? older.page.first;
  return {
    rows: [...newer.page.rows].reverse().concat(older.page.rows),
    ...(first === undefined ? {} : { first }),
    ...(older.page.last === undefined ? {} : { last: older.page.last }),
    hasOlder: older.page.hasMore,
    atNewest: !newer.page.hasMore,
    ...(newer.page.skipped + older.page.skipped > 0
      ? { notice: TimelineCopy.historySkipped }
      : {}),
  };
}

function lastIndex<T>(
  values: readonly T[],
  predicate: (value: T) => boolean,
): number {
  for (let index = values.length - 1; index >= 0; index -= 1) {
    const value = values[index];
    if (value !== undefined && predicate(value)) return index;
  }
  return -1;
}

function withoutUndefined<T extends object>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  ) as T;
}

/**
 * Recent windows of up to three conversations, 500 rows each, cleared when
 * the session ends (`TranscriptCache`).
 */
export class TranscriptCache {
  readonly #values = new Map<string, HistoryWindow>();

  get(id: string): HistoryWindow | undefined {
    const value = this.#values.get(id);
    if (value !== undefined) this.#touch(id, value);
    return value;
  }

  clear(): void {
    this.#values.clear();
  }

  /**
   * Stores `window` without duplicate IDs, trimmed to 500 rows centered on
   * `anchor`. Trimming the newest end clears `atNewest`; trimming the oldest
   * end sets `hasOlder`.
   */
  put(id: string, window: HistoryWindow, anchor?: string): HistoryWindow {
    const seen = new Set<string>();
    const rows = window.rows.filter((row) => {
      if (seen.has(row.id)) return false;
      seen.add(row.id);
      return true;
    });
    const found = rows.findIndex((row) => row.id === anchor);
    const index = found < 0 ? 0 : found;
    const start = Math.max(
      0,
      Math.min(index - CACHE_ROW_LIMIT / 2, rows.length - CACHE_ROW_LIMIT),
    );
    const kept = rows.slice(start, start + CACHE_ROW_LIMIT);
    let result: HistoryWindow = { ...window, rows: kept };
    if (start > 0) {
      result = withoutUndefined({
        ...result,
        first: kept[0]?.position,
        atNewest: false,
      });
    }
    if (start + kept.length < rows.length) {
      result = withoutUndefined({
        ...result,
        last: kept.at(-1)?.position,
        hasOlder: true,
      });
    }
    this.#touch(id, result);
    while (this.#values.size > CACHE_CONVERSATION_LIMIT) {
      const oldest = this.#values.keys().next();
      if (oldest.done === true) break;
      this.#values.delete(oldest.value);
    }
    return result;
  }

  /**
   * Appends an older page to the cached window (or to `base`, the window on
   * screen, when the cache evicted this conversation).
   */
  append(
    id: string,
    page: HistoryWindow,
    anchor?: string,
    base?: HistoryWindow,
  ): HistoryWindow {
    const old = this.#values.get(id) ?? base;
    const next: HistoryWindow =
      old === undefined
        ? page
        : withoutUndefined({
            ...page,
            rows: [...old.rows, ...page.rows],
            first: old.first,
            atNewest: old.atNewest,
            notice: old.notice ?? page.notice,
          });
    return this.put(id, next, anchor);
  }

  /**
   * Merges a re-read page into the cached window: the overlapping span is
   * replaced, rows outside it are kept. `base` stands in for an evicted entry.
   */
  refresh(
    id: string,
    page: HistoryWindow,
    anchor?: string,
    base?: HistoryWindow,
  ): HistoryWindow {
    const previous =
      this.#values.get(id) ??
      (base === undefined || base.rows.length === 0 ? undefined : base);
    if (previous === undefined) return this.put(id, page, anchor);
    const refreshed = new Set(page.rows.map((row) => row.id));
    const first = previous.rows.findIndex((row) => refreshed.has(row.id));
    const last = lastIndex(previous.rows, (row) => refreshed.has(row.id));
    if (first < 0 || last < 0) {
      return previous.atNewest && page.atNewest ? this.put(id, page) : previous;
    }
    const prefix = previous.rows.slice(0, first);
    const suffix = previous.rows.slice(last + 1);
    const previousIds = new Set(previous.rows.map((row) => row.id));
    let replacement = page.rows;
    if (!previous.atNewest) {
      const start = replacement.findIndex((row) => previousIds.has(row.id));
      if (start >= 0) replacement = replacement.slice(start);
    }
    if (previous.hasOlder) {
      const end = lastIndex(replacement, (row) => previousIds.has(row.id));
      if (end >= 0) replacement = replacement.slice(0, end + 1);
    }
    const next = withoutUndefined({
      ...page,
      rows: [...prefix, ...replacement, ...suffix],
      first:
        prefix.length === 0 && previous.atNewest ? page.first : previous.first,
      last:
        suffix.length === 0 && !previous.hasOlder ? page.last : previous.last,
      hasOlder:
        suffix.length === 0
          ? previous.hasOlder || page.hasOlder
          : previous.hasOlder,
      atNewest: previous.atNewest && (prefix.length > 0 || page.atNewest),
      notice: page.notice ?? previous.notice,
    });
    return this.put(id, next, anchor);
  }

  /** Shows a deleted message as deleted, without reactions. */
  markDeleted(conversationId: string, messageId: string): void {
    const window = this.#values.get(conversationId);
    if (window === undefined) return;
    if (!window.rows.some((row) => row.id === messageId)) return;
    this.#values.set(conversationId, {
      ...window,
      rows: window.rows.map((row) =>
        row.id === messageId ? deleted(row) : row,
      ),
    });
  }

  #touch(id: string, value: HistoryWindow): void {
    this.#values.delete(id);
    this.#values.set(id, value);
  }
}

/** The row shown as "Message deleted", without reactions. */
export function deleted(row: ChatMessage): ChatMessage {
  return { ...row, content: { kind: "deleted" }, reactions: [] };
}

/** A saved reading position. */
export interface ReadingAnchor {
  readonly messageId: string;
  readonly position: HistoryPosition;
}

/**
 * The row to scroll to for a saved anchor (`AnchorRestore.target`), given the
 * window rows newest first without reactions:
 * 1. the anchor row, if present and not deleted (unchanged);
 * 2. if present but deleted, the nearest newer surviving row, else the nearest
 *    older one (changed);
 * 3. if absent, the surviving row with the smallest sent time at or after the
 *    anchor's, else the largest before it (changed).
 */
export function anchorTarget(
  anchor: ReadingAnchor,
  rows: readonly ChatMessage[],
): { readonly id?: string; readonly changed: boolean } {
  if (
    rows.some((row) => row.id === anchor.messageId && !isDeleted(row.content))
  ) {
    return { id: anchor.messageId, changed: false };
  }
  const index = rows.findIndex((row) => row.id === anchor.messageId);
  if (index >= 0) {
    const newer = rows
      .slice(0, index)
      .reverse()
      .find((row) => !isDeleted(row.content));
    const older = rows.slice(index + 1).find((row) => !isDeleted(row.content));
    const id = newer?.id ?? older?.id;
    return id === undefined ? { changed: true } : { id, changed: true };
  }
  let newer: ChatMessage | undefined;
  let older: ChatMessage | undefined;
  for (const row of rows) {
    if (isDeleted(row.content)) continue;
    if (row.sentAtNs >= anchor.position.sentAtNs) {
      if (newer === undefined || row.sentAtNs < newer.sentAtNs) newer = row;
    } else if (older === undefined || row.sentAtNs > older.sentAtNs) {
      older = row;
    }
  }
  const id = newer?.id ?? older?.id;
  return id === undefined ? { changed: true } : { id, changed: true };
}

/**
 * The rows the chat shows, oldest first: published rows, then pending
 * (unpublished or failed) rows, both without reactions, each ID once.
 */
export function timelineMessages(
  window: HistoryWindow,
  pending: readonly ChatMessage[],
): readonly ChatMessage[] {
  const published = window.rows
    .filter((row) => row.content.kind !== "reaction")
    .reverse();
  // A row published since the pending page was read shows once, as published.
  const shown = new Set(published.map((row) => row.id));
  const queued = pending
    .filter((row) => row.content.kind !== "reaction" && !shown.has(row.id))
    .reverse();
  return published.concat(queued);
}

/** A date separator shows above the first row and where the local day changes. */
export function showsDateSeparator(
  previous: Date | undefined,
  current: Date,
): boolean {
  return (
    previous === undefined ||
    previous.getFullYear() !== current.getFullYear() ||
    previous.getMonth() !== current.getMonth() ||
    previous.getDate() !== current.getDate()
  );
}

/**
 * Read markers advance only when the app is in the foreground, the chat is
 * the current screen, the newest page is loaded and the newest row is visible.
 */
export function canMarkRead(conditions: {
  readonly foreground: boolean;
  readonly current: boolean;
  readonly windowAtNewest: boolean;
  readonly newestVisible: boolean;
}): boolean {
  return (
    conditions.foreground &&
    conditions.current &&
    conditions.windowAtNewest &&
    conditions.newestVisible
  );
}

/** The read marker of a window: its newest local insertion time. */
export function readMarker(window: HistoryWindow): bigint | undefined {
  let marker: bigint | undefined;
  for (const row of window.rows) {
    if (marker === undefined || row.insertedAtNs > marker) {
      marker = row.insertedAtNs;
    }
  }
  return marker;
}
