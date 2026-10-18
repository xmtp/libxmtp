// The Chats and Message requests lists (ExampleCore/Conversations/
// ConversationsStore.swift, ConversationModels.swift and
// ExampleSDK/ConversationQueries.swift `preview`).
//
// - `list` returns the conversations of one consent state by last activity
//   (the SDK sorts; the app does not re-sort). Reads carry IDs and kinds only.
// - Row details (title, preview, time, unread) are read lazily for the
//   visible rows plus 50 below them, a few at a time. A reload keeps the
//   details it has until the window reads them again.
// - Pages hold 50 rows. Loading more re-reads the whole prefix with a larger
//   limit. Message requests ask for extra rows to cover dismissed ones.
// - Chats also reads up to 501 requests for the "Message requests · N" badge.
// - Results apply only while the store is open, the session is current and
//   no newer load started.

import type { MessageBody, MessageContent } from "@xmtp/react-native-sdk";

import {
  CONVERSATION_PAGE_SIZE,
  REQUEST_COUNT_LIMIT,
  detailWindow,
  requestBadgeCount,
  shouldLoadNextPage,
} from "./requests.ts";
import { Store } from "./store.ts";
import {
  conversationInitials,
  conversationTitle,
  logicalKey,
  rowMatches,
  type TitleSource,
} from "./titles.ts";

export type ConversationConsent = "allowed" | "unknown" | "denied";

/** A conversation as the lists know it. Details are empty until read. */
export interface ConversationRecord extends TitleSource {
  readonly consent: ConversationConsent;
  readonly preview: string;
  readonly unread: number;
  readonly lastActivity?: Date;
}

/** A list row: the record and its display title and avatar initials. */
export interface ConversationRow {
  readonly id: string;
  readonly record: ConversationRecord;
  readonly title: string;
  readonly initials: string;
}

/** The SDK reads the lists need (`ConversationQueries`). */
export interface ConversationQueries {
  /** The conversations with `consent`, newest activity first, at most `limit`. */
  list(
    consent: ConversationConsent,
    limit: number,
  ): Promise<readonly ConversationRecord[]>;
  /**
   * The record with its details: consent, group name or DM peer and handle,
   * preview, last activity and the unread count after `marker(record)`.
   */
  details(
    record: ConversationRecord,
    marker: (record: ConversationRecord) => bigint,
  ): Promise<ConversationRecord>;
}

/** The per-profile values the lists read (`ConversationPreferences`). */
export interface ConversationListPreferences {
  alias(conversationId: string): string | undefined;
  /** The read marker (local insertion time in ns) for a logical key. */
  marker(logicalKey: string): bigint;
  readonly dismissedRequests: ReadonlySet<string>;
}

export interface ConversationListOptions {
  /** `allowed` for Chats, `unknown` for Message requests. */
  readonly consent: ConversationConsent;
  readonly ownInboxId: string;
  readonly queries: ConversationQueries;
  readonly preferences: ConversationListPreferences;
  /** False once the session that created the store ended. */
  readonly isCurrent: () => boolean;
  /** Detail reads at once. Default 4 (the iOS `AsyncLimiter(4)`). */
  readonly concurrency?: number;
}

export interface ConversationListState {
  /** The loaded rows, in SDK order. */
  readonly rows: readonly ConversationRow[];
  /** `rows` filtered by `search`. */
  readonly visible: readonly ConversationRow[];
  readonly search: string;
  readonly loading: boolean;
  /** False until the first list read finished. */
  readonly loaded: boolean;
  readonly hasMore: boolean;
  /** The request badge count; undefined when unknown or capped (Chats only). */
  readonly requestCount?: number;
  readonly error?: string;
}

export const LOAD_FAILED = "Cannot load conversations. Try again.";

/** The content types that count as unread (`unreadOptions`). */
export const UNREAD_CONTENT_KINDS = [
  "text",
  "markdown",
  "reply",
  "remoteAttachment",
] as const;

/** Visible rows read on first appearance (iOS `visibleCount = 10`). */
const INITIAL_VISIBLE_COUNT = 10;

function bodyText(body: MessageBody): string | undefined {
  return body.kind === "text" || body.kind === "markdown"
    ? body.value
    : undefined;
}

/**
 * A row preview for the last message: text and markdown bodies, a reply's
 * text body (else its fallback or "Reply"), "Attachment" for a remote
 * attachment, else the fallback or nothing.
 */
export function previewText(
  content: MessageContent,
  fallback: string | undefined,
): string {
  switch (content.kind) {
    case "text":
    case "markdown":
      return content.value;
    case "reply":
      return bodyText(content.body) ?? fallback ?? "Reply";
    case "remoteAttachment":
      return "Attachment";
    default:
      return fallback ?? "";
  }
}

/** Rows that match `search` (title, peer handle, preview or ID), all rows when empty. */
export function filterRows(
  rows: readonly ConversationRow[],
  search: string,
): readonly ConversationRow[] {
  if (search === "") return rows;
  return rows.filter((row) =>
    rowMatches(
      {
        title: row.title,
        ...(row.record.peerHandle === undefined
          ? {}
          : { peerHandle: row.record.peerHandle }),
        preview: row.record.preview,
        id: row.id,
      },
      search,
    ),
  );
}

export class ConversationListStore extends Store<ConversationListState> {
  readonly #options: ConversationListOptions;
  readonly #concurrency: number;
  #limit = CONVERSATION_PAGE_SIZE;
  #visibleStart = 0;
  #visibleCount = INITIAL_VISIBLE_COUNT;
  #loadRevision = 0;
  #windowRevision = 0;
  /** The revision of the running list read, if it is the newest. */
  #loadInFlight: number | undefined;
  #queue: string[] = [];
  readonly #queued = new Set<string>();
  readonly #hydrated = new Set<string>();
  #active = 0;
  #disposed = false;
  #idleWaiters: (() => void)[] = [];

  constructor(options: ConversationListOptions) {
    super({
      rows: [],
      visible: [],
      search: "",
      loading: false,
      loaded: false,
      hasMore: true,
    });
    this.#options = options;
    this.#concurrency = Math.max(1, options.concurrency ?? 4);
  }

  /** True after `dispose`; a closed store never changes again. */
  get disposed(): boolean {
    return this.#disposed;
  }

  get consent(): ConversationConsent {
    return this.#options.consent;
  }

  /** Reads the first page again (`.load`). */
  load(): void {
    this.#limit = CONVERSATION_PAGE_SIZE;
    void this.#load();
  }

  /**
   * Re-reads the loaded pages from the local database and waits for the
   * list and its window details (pull to refresh, stream updates).
   */
  async refresh(): Promise<void> {
    await this.#load();
    await this.whenIdle();
  }

  /** Reads one more page, unless a search runs, a load runs, or no more rows exist. */
  loadMore(): void {
    const state = this.getState();
    if (state.search !== "" || state.loading || !state.hasMore) return;
    this.#limit += CONVERSATION_PAGE_SIZE;
    void this.#load();
  }

  setSearch(text: string): void {
    if (this.#disposed) return;
    this.setState((state) => ({
      ...state,
      search: text,
      visible: filterRows(state.rows, text),
    }));
    this.#visibleStart = 0;
    this.#resetHydration();
    this.#schedule();
  }

  /**
   * Visible rows changed (`.visible(start:count:)`): read their details and
   * the 50 below, and load the next page near the end.
   */
  visible(start: number, count: number): void {
    if (this.#disposed) return;
    this.#visibleStart = Math.max(0, start);
    this.#visibleCount = Math.max(0, count);
    this.#schedule();
    const state = this.getState();
    if (
      shouldLoadNextPage(
        start + Math.max(0, count - 1),
        state.rows.length,
        state.search,
      )
    ) {
      this.loadMore();
    }
  }

  /** Resolves once no list or detail read is running. */
  whenIdle(): Promise<void> {
    if (this.#idle()) return Promise.resolve();
    return new Promise((resolve) => {
      this.#idleWaiters.push(resolve);
    });
  }

  dispose(): void {
    this.#disposed = true;
    this.#resetHydration();
    this.#loadRevision += 1;
    this.#releaseIdle(true);
  }

  #idle(): boolean {
    return (
      this.#loadInFlight === undefined &&
      this.#active === 0 &&
      this.#queue.length === 0
    );
  }

  #releaseIdle(force = false): void {
    if (!force && !this.#idle()) return;
    const waiters = this.#idleWaiters;
    this.#idleWaiters = [];
    for (const waiter of waiters) waiter();
  }

  #open(): boolean {
    return !this.#disposed && this.#options.isCurrent();
  }

  #dismissed(): ReadonlySet<string> {
    try {
      return this.#options.preferences.dismissedRequests;
    } catch {
      return new Set();
    }
  }

  #row(record: ConversationRecord): ConversationRow {
    let alias: string | undefined;
    try {
      alias = this.#options.preferences.alias(record.id);
    } catch {
      alias = undefined;
    }
    const title = conversationTitle(record, alias);
    return {
      id: record.id,
      record,
      title,
      initials: conversationInitials(record, title),
    };
  }

  #marker(record: ConversationRecord): bigint {
    try {
      return this.#options.preferences.marker(
        logicalKey(record, this.#options.ownInboxId),
      );
    } catch {
      return 0n;
    }
  }

  async #countRequests(): Promise<readonly string[] | undefined> {
    // Only Chats shows the badge; Requests already read its list.
    if (this.#options.consent !== "allowed") return undefined;
    try {
      const records = await this.#options.queries.list(
        "unknown",
        REQUEST_COUNT_LIMIT,
      );
      return records.map((record) => record.id);
    } catch {
      return undefined;
    }
  }

  async #load(): Promise<void> {
    if (!this.#open()) return;
    this.#loadRevision += 1;
    const revision = this.#loadRevision;
    const before = this.getState();
    const retained = before.visible
      .filter((row) => this.#queued.has(row.id))
      .map((row) => row.id);
    this.#resetHydration();
    this.setState((state) => ({ ...state, loading: true, error: undefined }));
    const { consent, queries } = this.#options;
    const limit = this.#limit;
    // Dismissed requests still occupy the SDK prefix: read enough for a full page.
    const requested =
      limit + (consent === "unknown" ? this.#dismissed().size : 0);
    this.#loadInFlight = revision;
    try {
      const records = await queries.list(consent, requested);
      const requests = await this.#countRequests();
      if (!this.#open() || revision !== this.#loadRevision) return;
      const dismissed = this.#dismissed();
      const existing = new Map(
        this.getState().rows.map((row) => [row.id, row.record]),
      );
      const shown = records.filter(
        (record) =>
          record.consent === consent &&
          (consent !== "unknown" || !dismissed.has(record.id)),
      );
      const rows = shown.slice(0, limit).map((record) => {
        // List reads carry IDs only: keep known details until the window reads them again.
        const known = existing.get(record.id);
        return this.#row(
          known === undefined ? record : { ...known, consent: record.consent },
        );
      });
      const ids = new Set(rows.map((row) => row.id));
      for (const id of this.#hydrated) {
        if (!ids.has(id)) this.#hydrated.delete(id);
      }
      const requestCount =
        requests === undefined
          ? undefined
          : requestBadgeCount(requests, dismissed);
      this.setState((state) => ({
        ...state,
        rows,
        visible: filterRows(rows, state.search),
        loading: false,
        loaded: true,
        hasMore: records.length === requested || shown.length > limit,
        requestCount,
        error: undefined,
      }));
      this.#schedule(retained);
    } catch {
      if (!this.#open() || revision !== this.#loadRevision) return;
      this.setState((state) => ({
        ...state,
        loading: false,
        loaded: true,
        error: LOAD_FAILED,
      }));
    } finally {
      if (this.#loadInFlight === revision) this.#loadInFlight = undefined;
      this.#releaseIdle();
    }
  }

  #resetHydration(): void {
    this.#windowRevision += 1;
    this.#queue = [];
    this.#queued.clear();
    this.#active = 0;
  }

  #enqueue(id: string): void {
    if (this.#queued.has(id)) return;
    this.#queued.add(id);
    this.#queue.push(id);
  }

  #schedule(retained: readonly string[] = []): void {
    if (!this.#open()) return;
    const state = this.getState();
    const displayed = state.visible;
    const displayedIds = new Set(displayed.map((row) => row.id));
    for (const id of retained) {
      if (displayedIds.has(id)) this.#enqueue(id);
    }
    const [lower, upper] = detailWindow(
      this.#visibleStart,
      this.#visibleCount,
      displayed.length,
    );
    for (const row of displayed.slice(lower, upper)) this.#enqueue(row.id);
    if (state.search !== "") {
      // Search cannot find a field until it is read: read the loaded window too.
      const [from, to] = detailWindow(
        this.#visibleStart,
        this.#visibleCount,
        state.rows.length,
      );
      for (const row of state.rows.slice(from, to)) {
        if (!this.#hydrated.has(row.id)) this.#enqueue(row.id);
      }
    }
    this.#pump();
    this.#releaseIdle();
  }

  #pump(): void {
    while (this.#active < this.#concurrency && this.#queue.length > 0) {
      const id = this.#queue.shift();
      if (id === undefined) break;
      this.#active += 1;
      void this.#hydrate(id, this.#windowRevision, this.#loadRevision);
    }
  }

  async #hydrate(
    id: string,
    windowRevision: number,
    loadRevision: number,
  ): Promise<void> {
    const current = () =>
      this.#open() &&
      windowRevision === this.#windowRevision &&
      loadRevision === this.#loadRevision;
    try {
      const row = this.getState().rows.find((entry) => entry.id === id);
      if (row === undefined || !current()) return;
      const record = await this.#options.queries.details(row.record, (value) =>
        this.#marker(value),
      );
      if (!current()) return;
      const consent = this.#options.consent;
      this.setState((state) => {
        const index = state.rows.findIndex((entry) => entry.id === record.id);
        if (index < 0) return state;
        const rows = [...state.rows];
        if (record.consent === consent) rows[index] = this.#row(record);
        else rows.splice(index, 1);
        return { ...state, rows, visible: filterRows(rows, state.search) };
      });
      if (record.consent === consent) this.#hydrated.add(record.id);
      else this.#hydrated.delete(record.id);
    } catch {
      // Keep the row as it is; the next load reads it again.
    } finally {
      if (windowRevision === this.#windowRevision) {
        this.#queued.delete(id);
        this.#active -= 1;
        this.#pump();
      }
      this.#releaseIdle();
    }
  }
}
