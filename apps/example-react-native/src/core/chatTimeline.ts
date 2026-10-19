// The chat timeline of one open chat.
//
// It loads published history in pages of 50 (newest first, or around the
// saved reading anchor), the pending (unpublished or failed) rows after them,
// merges stream refreshes through the transcript cache, saves the topmost
// visible row as the reading anchor, and advances the read marker. A new
// action cancels the one in flight; results apply only while the chat is open
// in the current session (`admit`).
//
// Restoring the anchor reads one `around` window; when no surviving row is in
// it, the newest page loads with the "Position changed" notice.

import {
  type ChatQueries,
  EMPTY_WINDOW,
  type HistoryWindow,
  type ReadingAnchor,
  TimelineCopy,
  type TranscriptCache,
  anchorTarget,
  aroundWindow,
  canMarkRead,
  deleted,
  olderWindow,
  readMarker,
  timelineMessages,
} from "./chatHistory.ts";
import {
  type ChatMessage,
  type ConversationRecord,
  type RecoveryPosition,
  isDeleted,
} from "./chatMessages.ts";
import { errorLogText } from "./errorLog.ts";
import { Store } from "./store.ts";

export interface TimelineState {
  readonly conversation?: ConversationRecord;
  readonly window: HistoryWindow;
  /** Unpublished and failed rows, newest first. */
  readonly pending: readonly ChatMessage[];
  readonly pendingBefore?: RecoveryPosition;
  readonly morePending: boolean;
  readonly loading: boolean;
  /** True after the first load finished (or failed). */
  readonly loaded: boolean;
  readonly error?: string;
  /** A restored reading position: scroll this row to the center once. */
  readonly target?: string;
  /** Increments with each new target, so the view scrolls again. */
  readonly targetRevision: number;
  /** The topmost visible row (the reading anchor). */
  readonly anchorId?: string;
  /** False in reading mode: new messages do not scroll the list. */
  readonly atNewest: boolean;
  /** The rows to show, oldest first (published, then pending). */
  readonly messages: readonly ChatMessage[];
}

export interface TimelinePorts {
  readonly conversationId: string;
  readonly queries: Pick<ChatQueries, "conversation" | "history" | "pending">;
  readonly cache: TranscriptCache;
  /** False once the chat screen closed or its session ended. */
  readonly admit: () => boolean;
  /** The saved reading position; undefined when none or unreadable. */
  readonly readAnchor: () => ReadingAnchor | undefined;
  /** Saves the reading position. Throws when it cannot. */
  readonly saveAnchor: (anchor: ReadingAnchor) => void;
  /** Advances the read marker. Throws when it cannot. */
  readonly markRead: (conversation: ConversationRecord, marker: bigint) => void;
}

type Action = "load" | "older" | "olderPending" | "newest" | "refresh";

/**
 * Pending pages one load reads while each holds only unreadable rows. After
 * that many, the load stops and offers "Older pending messages".
 */
export const PENDING_READ_ATTEMPTS = 4;

/** A result that arrived after a newer action or after the chat closed. */
class Superseded extends Error {
  constructor() {
    super("Superseded");
    this.name = "Superseded";
  }
}

interface Target {
  readonly id?: string;
  readonly changed: boolean;
}

function initialState(cached: HistoryWindow | undefined): TimelineState {
  const window = cached ?? EMPTY_WINDOW;
  return {
    window,
    pending: [],
    morePending: false,
    loading: false,
    loaded: cached !== undefined,
    targetRevision: 0,
    atNewest: true,
    messages: timelineMessages(window, []),
  };
}

function sameAnchor(
  left: ReadingAnchor | undefined,
  right: ReadingAnchor,
): boolean {
  return (
    left !== undefined &&
    left.messageId === right.messageId &&
    left.position.sentAtNs === right.position.sentAtNs &&
    left.position.cursor === right.position.cursor
  );
}

export class ChatTimeline extends Store<TimelineState> {
  readonly #ports: TimelinePorts;
  #revision = 0;
  #job: Promise<void> = Promise.resolve();
  #savedAnchor: ReadingAnchor | undefined;
  #savedMarker: bigint | undefined;
  /** Unreadable rows in the loaded pending pages. */
  #pendingSkipped = 0;

  constructor(ports: TimelinePorts) {
    // A cached window shows at once while the first load runs.
    super(initialState(ports.cache.get(ports.conversationId)));
    this.#ports = ports;
  }

  /** Opens the chat: around the saved reading position, else the newest page. */
  load(): Promise<void> {
    return this.#start("load");
  }

  /** "Older messages": the page before the oldest loaded row. */
  older(): Promise<void> {
    return this.#start("older");
  }

  /** "Older pending messages": adds the page before the oldest pending row. */
  olderPending(): Promise<void> {
    return this.#start("olderPending");
  }

  /** "Jump to latest": leaves reading mode and loads the newest page. */
  newest(): Promise<void> {
    this.#apply((state) => {
      const { target: _target, ...rest } = state;
      return { ...rest, atNewest: true };
    });
    return this.#start("newest");
  }

  /** Re-reads what is shown (stream messages, sends, consent changes). */
  refresh(): Promise<void> {
    return this.#start("refresh");
  }

  /** Resolves when the action in flight settles. */
  wait(): Promise<void> {
    return this.#job;
  }

  /**
   * The visible rows changed. While a restore target is set, nothing happens
   * until the target is visible. The topmost visible surviving row becomes the
   * reading anchor; the read marker advances when `canMarkRead` holds.
   */
  visible(
    ids: readonly string[],
    conditions: { readonly foreground: boolean; readonly current: boolean },
  ): void {
    if (!this.#ports.admit()) return;
    const visible = new Set(ids);
    if (visible.size === 0) return;
    const state = this.getState();
    if (state.target !== undefined) {
      if (!visible.has(state.target)) return;
      this.setState((current) => {
        const { target: _target, ...rest } = current;
        return rest;
      });
    }
    const { messages } = this.getState();
    const anchor = messages.find(
      (row) => visible.has(row.id) && !isDeleted(row.content),
    )?.id;
    const last = messages.at(-1);
    const newest = last !== undefined && visible.has(last.id);
    this.#updateReadingPosition(anchor, newest, conditions);
  }

  /** A message was deleted: show it as deleted at once, even if a read is in flight. */
  messageDeleted(messageId: string): void {
    if (!this.#ports.admit()) return;
    this.#revision += 1;
    this.#ports.cache.markDeleted(this.#ports.conversationId, messageId);
    this.setState((state) => {
      const redact = (row: ChatMessage) =>
        row.id === messageId ? deleted(row) : row;
      const window = { ...state.window, rows: state.window.rows.map(redact) };
      const pending = state.pending.map(redact);
      return {
        ...state,
        window,
        pending,
        loading: false,
        messages: timelineMessages(window, pending),
      };
    });
  }

  #start(action: Action): Promise<void> {
    if (!this.#ports.admit()) return Promise.resolve();
    this.#revision += 1;
    const revision = this.#revision;
    this.setState((state) => ({ ...state, loading: true }));
    const job = this.#run(action, revision);
    this.#job = job;
    return job;
  }

  #apply(change: (state: TimelineState) => TimelineState): void {
    if (!this.#ports.admit()) return;
    this.setState(change);
  }

  async #run(action: Action, revision: number): Promise<void> {
    const check = (): void => {
      if (revision !== this.#revision || !this.#ports.admit()) {
        throw new Superseded();
      }
    };
    const { queries, conversationId: id } = this.#ports;
    try {
      const conversation = await queries.conversation(id);
      check();
      if (action === "olderPending") {
        await this.#loadPending(this.getState().pendingBefore, check);
        this.#apply((state) => ({
          ...state,
          conversation,
          loading: false,
          loaded: true,
        }));
        return;
      }
      const anchor = this.#readingAnchor(action);
      let window = await this.#historyWindow(action, anchor, check);
      let target: Target | undefined;
      if (anchor !== undefined) {
        target = anchorTarget(
          anchor,
          window.rows.filter((row) => row.content.kind !== "reaction"),
        );
        if (action === "refresh" && !target.changed) target = undefined;
        if (target?.changed === true) {
          window = { ...window, notice: TimelineCopy.positionChanged };
        }
        if (target !== undefined && target.id === undefined) {
          // No surviving row near the anchor: show the newest page instead.
          const newest = await olderWindow(queries, id, undefined, check);
          window = { ...newest, notice: TimelineCopy.positionChanged };
          target = undefined;
        }
      }
      check();
      const state = this.getState();
      window =
        action === "refresh" && target?.changed !== true
          ? this.#ports.cache.refresh(id, window, state.anchorId, state.window)
          : this.#ports.cache.put(
              id,
              window,
              anchor?.messageId ?? state.anchorId,
            );
      const preserveNewest =
        action === "refresh" && state.atNewest && state.window.rows.length > 0;
      this.#publish(window, conversation, target, preserveNewest);
      await this.#loadPending(undefined, check);
    } catch (error) {
      if (error instanceof Superseded) return;
      if (revision !== this.#revision) return;
      console.warn("Cannot load the chat timeline", errorLogText(error));
      this.#apply((state) => ({
        ...state,
        loading: false,
        loaded: true,
        error: TimelineCopy.updatesStopped,
      }));
    }
  }

  #readingAnchor(action: Action): ReadingAnchor | undefined {
    const state = this.getState();
    if (action === "load") return this.#readSavedAnchor();
    if (action !== "refresh") return undefined;
    const row = state.window.rows.find((entry) => entry.id === state.anchorId);
    if (row?.position !== undefined) {
      return { messageId: row.id, position: row.position };
    }
    if (state.window.rows.length === 0) return this.#readSavedAnchor();
    return undefined;
  }

  #readSavedAnchor(): ReadingAnchor | undefined {
    try {
      const anchor = this.#ports.readAnchor();
      this.#savedAnchor = anchor;
      return anchor;
    } catch {
      return undefined;
    }
  }

  async #historyWindow(
    action: Action,
    anchor: ReadingAnchor | undefined,
    check: () => void,
  ): Promise<HistoryWindow> {
    const { queries, conversationId: id, cache } = this.#ports;
    const state = this.getState();
    if (action === "older") {
      const page = await olderWindow(queries, id, state.window.last, check);
      check();
      return cache.append(
        id,
        page,
        state.window.rows.at(-1)?.id,
        this.getState().window,
      );
    }
    if (
      anchor !== undefined &&
      (action === "load" || state.window.rows.length === 0)
    ) {
      return aroundWindow(queries, id, anchor.position, check);
    }
    if (action === "refresh" && (!state.window.atNewest || !state.atNewest)) {
      const position =
        state.window.rows.find((row) => row.id === state.anchorId)?.position ??
        state.window.first;
      if (position !== undefined) {
        return aroundWindow(queries, id, position, check);
      }
    }
    return olderWindow(queries, id, undefined, check);
  }

  /**
   * Reads the pending page before `before` and adds it to the pending rows;
   * without `before`, reads the newest page and replaces them.
   */
  async #loadPending(
    before: RecoveryPosition | undefined,
    check: () => void,
  ): Promise<void> {
    const { queries, conversationId: id } = this.#ports;
    let cursor = before;
    let skipped = before === undefined ? 0 : this.#pendingSkipped;
    for (let attempt = 0; attempt < PENDING_READ_ATTEMPTS; attempt += 1) {
      check();
      const page = await queries.pending(id, cursor);
      check();
      skipped += page.skipped;
      if (page.rows.length > 0 || !page.hasMore) {
        this.#pendingSkipped = skipped;
        this.#apply((state) => {
          const { error: _error, pendingBefore: _before, ...rest } = state;
          const pending =
            before === undefined ? page.rows : [...state.pending, ...page.rows];
          const error =
            skipped > 0
              ? page.hasMore
                ? TimelineCopy.pendingSkippedMore
                : TimelineCopy.pendingSkippedRefresh
              : undefined;
          return {
            ...rest,
            pending,
            ...(page.last === undefined ? {} : { pendingBefore: page.last }),
            morePending: page.hasMore,
            ...(error === undefined ? {} : { error }),
            messages: timelineMessages(state.window, pending),
          };
        });
        return;
      }
      cursor = page.last;
      if (cursor === undefined) throw new Error("No pending position.");
    }
    const last = cursor;
    this.#pendingSkipped = skipped;
    this.#apply((state) => {
      const pending = before === undefined ? [] : state.pending;
      return {
        ...state,
        pending,
        ...(last === undefined ? {} : { pendingBefore: last }),
        morePending: true,
        ...(skipped > 0 ? { error: TimelineCopy.pendingSkippedMore } : {}),
        messages: timelineMessages(state.window, pending),
      };
    });
  }

  #publish(
    window: HistoryWindow,
    conversation: ConversationRecord,
    target: Target | undefined,
    preserveNewest: boolean,
  ): void {
    this.#apply((state) => {
      const { error: _error, ...rest } = state;
      let next: TimelineState = {
        ...rest,
        conversation,
        window,
        loading: false,
        loaded: true,
        messages: timelineMessages(window, state.pending),
      };
      if (target !== undefined) {
        const { target: _old, ...withoutTarget } = next;
        next = {
          ...withoutTarget,
          ...(target.id === undefined ? {} : { anchorId: target.id }),
          ...(preserveNewest || target.id === undefined
            ? {}
            : { target: target.id, targetRevision: state.targetRevision + 1 }),
          atNewest: preserveNewest,
        };
      }
      return next;
    });
  }

  #updateReadingPosition(
    id: string | undefined,
    newest: boolean,
    conditions: { readonly foreground: boolean; readonly current: boolean },
  ): void {
    const state = this.getState();
    const row =
      id === undefined
        ? undefined
        : state.window.rows.find((entry) => entry.id === id);
    if (row !== undefined && isDeleted(row.content)) return;
    // Scrolling reports visibility every frame: an unchanged position keeps
    // the same state, so nothing re-renders.
    this.setState((current) =>
      current.atNewest === newest &&
      (id === undefined || current.anchorId === id)
        ? current
        : {
            ...current,
            atNewest: newest,
            ...(id === undefined ? {} : { anchorId: id }),
          },
    );
    try {
      if (row?.position !== undefined) {
        const anchor = { messageId: row.id, position: row.position };
        if (!sameAnchor(this.#savedAnchor, anchor)) {
          this.#ports.saveAnchor(anchor);
          this.#savedAnchor = anchor;
        }
      }
      const marker = readMarker(state.window);
      if (
        state.conversation !== undefined &&
        marker !== undefined &&
        marker !== this.#savedMarker &&
        canMarkRead({
          ...conditions,
          windowAtNewest: state.window.atNewest,
          newestVisible: newest,
        })
      ) {
        this.#ports.markRead(state.conversation, marker);
        this.#savedMarker = marker;
      }
    } catch {
      this.setState((current) => ({
        ...current,
        error: TimelineCopy.readingPositionFailed,
      }));
    }
  }
}
