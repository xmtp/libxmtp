// The stores of one open chat and the actions the chat's views call.
//
// The screen creates one runtime per conversation and session, then attaches
// it while mounted: attaching registers the stream listeners and loads;
// detaching drops every later result. The chat stays mounted behind sheets
// and pushed screens (plan decision D12), so it keeps updating there.

import { ChatComposer } from "@/core/chatComposer.ts";
import {
  type ReadingAnchor,
  type TranscriptCache,
} from "@/core/chatHistory.ts";
import { type ChatMessage, contentText } from "@/core/chatMessages.ts";
import { ChatTimeline } from "@/core/chatTimeline.ts";
import { MessageActions } from "@/core/messageActions.ts";
import { ValueStore } from "@/core/store.ts";
import { logicalKey } from "@/core/titles.ts";
import { type ChatServices, chatServices } from "@/features/chat/chatServices";
import type { ActiveSession } from "@/session";

/**
 * The attached runtimes of each open chat, in attach order. A conversation
 * can be open twice: Settings › Send needs review › View chat pushes it again
 * over the chat that opened Settings.
 */
const mounted = new Map<string, readonly ChatRuntime[]>();

/**
 * The open chat of `conversationId` in the current session, if it is mounted
 * (the chat stays mounted behind its sheets and pushed screens). When it is
 * open twice, the most recently attached one. Sheets such as Reactions and
 * the emoji picker can act through it.
 */
export function mountedChat(
  conversationId: string,
  session: ActiveSession,
): ChatRuntime | undefined {
  return (mounted.get(conversationId) ?? [])
    .filter((runtime) => runtime.session === session)
    .at(-1);
}

export class ChatRuntime {
  readonly conversationId: string;
  readonly session: ActiveSession;
  readonly services: ChatServices;
  readonly timeline: ChatTimeline;
  readonly composer: ChatComposer;
  /** Retry, react and delete for rows of this chat. */
  readonly actions: MessageActions;
  /**
   * The last text the user submitted, with an increasing count, for the send
   * flight: when the newest row is own with this text, it flies from the
   * composer.
   */
  readonly submissions = new ValueStore<
    { readonly text: string; readonly count: number } | undefined
  >(undefined);
  #attached = 0;
  #stops: (() => void)[] = [];
  readonly #requestedSenders = new Set<string>();

  constructor(session: ActiveSession, conversationId: string) {
    this.session = session;
    this.conversationId = conversationId;
    const services = chatServices(session);
    this.services = services;
    const admit = (): boolean => this.#attached > 0 && session.isCurrent();
    const preferences = session.preferences;
    this.timeline = new ChatTimeline({
      conversationId,
      queries: services.queries,
      cache: services.cache,
      admit,
      readAnchor: (): ReadingAnchor | undefined => {
        const saved = preferences.scrollAnchor(conversationId);
        return saved === undefined
          ? undefined
          : {
              messageId: saved.messageId,
              position: { sentAtNs: saved.sentAtNs, cursor: saved.cursor },
            };
      },
      saveAnchor: (anchor) => {
        preferences.setScrollAnchor(conversationId, {
          messageId: anchor.messageId,
          sentAtNs: anchor.position.sentAtNs,
          cursor: anchor.position.cursor,
        });
      },
      markRead: (conversation, marker) => {
        preferences.setMarker(
          logicalKey(conversation, session.inboxId),
          marker,
        );
      },
    });
    const refresh = (): Promise<void> => this.timeline.refresh();
    this.composer = new ChatComposer({
      conversationId,
      drafts: {
        load: (id) => preferences.composerDraft(id),
        save: (id, draft) => {
          preferences.saveComposerDraft(id, draft);
        },
      },
      journal: services.journal,
      coordinator: services.coordinator,
      queries: services.queries,
      admit,
      accepted: refresh,
      newId: services.newId,
      now: services.now,
      run: services.run,
    });
    this.actions = new MessageActions({
      queries: services.queries,
      coordinator: services.coordinator,
      own: session.inboxId,
      admit,
      changed: refresh,
      newId: services.newId,
      now: services.now,
      run: services.run,
    });
  }

  /** The own inbox ID. */
  get own(): string {
    return this.session.inboxId;
  }

  /** The conversation's consent is allowed (replies, reactions, sends). */
  get allowed(): boolean {
    return this.timeline.getState().conversation?.consent === "allowed";
  }

  /** The transcript cache of this session (shared by its chats). */
  get cache(): TranscriptCache {
    return this.services.cache;
  }

  /**
   * Starts updates and loads the timeline. Returns the detach function. A
   * detached runtime ignores every result, so a remount attaches again.
   */
  attach(): () => void {
    const { conversationId } = this;
    this.#attached += 1;
    if (this.#attached === 1) this.#start();
    mounted.set(conversationId, [...(mounted.get(conversationId) ?? []), this]);
    let detached = false;
    return () => {
      if (detached) return;
      detached = true;
      this.#attached -= 1;
      if (this.#attached === 0) this.#stop();
      // Drop this attachment only: a copy of the chat lower in the stack
      // stays reachable through `mountedChat`.
      const runtimes = mounted.get(conversationId) ?? [];
      const index = runtimes.lastIndexOf(this);
      const rest = runtimes.filter((_, position) => position !== index);
      if (rest.length === 0) mounted.delete(conversationId);
      else mounted.set(conversationId, rest);
    };
  }

  #start(): void {
    const { session, conversationId } = this;
    try {
      this.services.coordinator.recoverAccepted();
    } catch (error) {
      console.warn("Cannot recover an accepted send", error);
    }
    this.#stops = [
      this.timeline.subscribe(() => {
        this.#consentChanged();
        this.#loadSenders();
      }),
      session.streams.onMessage((message) => {
        if (message.conversationId !== conversationId) return;
        this.composer.reload();
        void this.timeline.refresh();
      }),
      session.streams.onEvent((event) => {
        if (event.kind === "messageDeleted") {
          if (event.conversationId === conversationId) {
            this.timeline.messageDeleted(event.messageId);
          }
        } else if (event.kind === "changed") {
          void this.timeline.refresh();
        }
      }),
    ];
    this.composer.reload();
    this.#consentChanged();
    void this.timeline.load();
  }

  #stop(): void {
    for (const stop of this.#stops) stop();
    this.#stops = [];
  }

  #consentChanged(): void {
    const allowed = this.allowed;
    this.composer.allow(allowed);
    this.actions.allowed = allowed;
  }

  /** Reads the handles of group senders the directory does not know yet. */
  #loadSenders(): void {
    const { identities } = this.session;
    const missing: string[] = [];
    for (const row of this.timeline.getState().messages) {
      if (row.sender === this.own || this.#requestedSenders.has(row.sender)) {
        continue;
      }
      this.#requestedSenders.add(row.sender);
      if (identities.identity(row.sender) === undefined)
        missing.push(row.sender);
    }
    if (missing.length === 0) return;
    identities.load(missing).catch((error: unknown) => {
      console.warn("Cannot read sender handles", error);
      for (const sender of missing) this.#requestedSenders.delete(sender);
    });
  }

  // Actions for the chat's views (context menu, swipe, sheets). Each takes a
  // row ID of this chat; unknown IDs do nothing.

  /** The row with `messageId`, if it is loaded. */
  message(messageId: string): ChatMessage | undefined {
    return this.timeline
      .getState()
      .messages.find((row) => row.id === messageId);
  }

  /** The text Copy puts on the pasteboard ("Message deleted", a file name). */
  copyText(messageId: string): string | undefined {
    const row = this.message(messageId);
    return row === undefined ? undefined : contentText(row.content);
  }

  /** Reply: the composer shows "Reply to …" and sends a reply. */
  startReply(messageId: string): void {
    this.composer.reply(messageId);
  }

  /** Send: submits the composer text and records it in `submissions`. */
  submit(): void {
    const text = this.composer.getState().draft.text;
    const count = (this.submissions.getState()?.count ?? 0) + 1;
    this.submissions.set({ text, count });
    this.composer.submit();
  }

  /** "Cancel reply". */
  cancelReply(): void {
    this.composer.reply(undefined);
  }

  /** Retry: publishes the same message ID again (own unpublished rows). */
  retry(messageId: string): void {
    const row = this.message(messageId);
    if (row !== undefined) this.actions.retry(row);
  }

  /** Edit: copies the row's text into the composer (replaces the draft text). */
  edit(messageId: string): void {
    const row = this.message(messageId);
    if (row !== undefined) this.composer.edit(contentText(row.content));
  }

  /** Toggles the own reaction `emoji` on a row. */
  react(messageId: string, emoji: string): void {
    const row = this.message(messageId);
    if (row !== undefined) this.actions.react(row, emoji);
  }

  /** Sets `actions.deleteTarget` for a "Delete message?" confirmation. */
  requestDelete(messageId: string): void {
    const row = this.message(messageId);
    if (row !== undefined) this.actions.requestDelete(row);
  }

  /** Deletes an own published row now (callers confirm first). */
  deleteMessage(messageId: string): void {
    const row = this.message(messageId);
    if (row !== undefined) this.actions.deleteMessage(row);
  }
}

/**
 * A message action store for a screen over the chat (the Reactions sheet,
 * the emoji picker): `load(messageId)`, then `react(message, emoji)`. Its
 * changes refresh the chat of `conversationId` when it is mounted.
 */
export function createMessageActions(
  session: ActiveSession,
  conversationId: string,
  admit: () => boolean,
): MessageActions {
  const services = chatServices(session);
  return new MessageActions({
    queries: services.queries,
    coordinator: services.coordinator,
    own: session.inboxId,
    admit: () => admit() && session.isCurrent(),
    changed: async () => {
      await mountedChat(conversationId, session)?.timeline.refresh();
    },
    newId: services.newId,
    now: services.now,
    run: services.run,
  });
}
