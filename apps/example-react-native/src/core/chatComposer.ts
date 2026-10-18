// The composer of one open chat (ExampleCore/Chat/ComposerStore.swift,
// Send/ComposerDraftStore.swift).
//
// Every edit is saved at once. A send reserves the draft (its request ID and
// the submitted revision) and saves it, journals a `queueing` entry before
// the SDK call, saves the accepted message ID, then clears the text only if
// it was not edited meanwhile, and publishes. A failure that is not a known
// rejection leaves the journal entry, so the outcome is unknown: the composer
// locks until Settings › Send needs review (or `clearReview`) clears it.

import type { ChatQueries, OutboundContent } from "./chatHistory.ts";
import type { ComposerDraft } from "./preferences.ts";
import {
  type SendCoordinator,
  type SendJournal,
  composerOutcomeUnknown,
} from "./sendJournal.ts";
import { Store } from "./store.ts";

export const ComposerCopy = {
  saveFailed: "Cannot save your draft. Try again.",
  loadFailed: "Cannot load your saved draft.",
  clearFailed: "Cannot clear saved send status.",
  acceptedPending: "Send accepted. Local recovery is pending.",
  outcomeUnknown:
    "Send outcome unknown. The original message may already exist.",
  rejected: "Cannot send this message. Your draft is kept.",
} as const;

export const EMPTY_DRAFT: ComposerDraft = { text: "", revision: 0 };

/** Replaces the text; the revision increments only when it changes. */
export function editDraft(draft: ComposerDraft, text: string): ComposerDraft {
  return draft.text === text
    ? draft
    : { ...draft, text, revision: draft.revision + 1 };
}

/** Drops the send reservation (request ID, submitted revision, accepted ID). */
export function releaseDraft(draft: ComposerDraft): ComposerDraft {
  const {
    requestId: _request,
    submittedRevision: _submitted,
    acceptedId: _accepted,
    ...rest
  } = draft;
  return rest;
}

function withReply(
  draft: ComposerDraft,
  replyId: string | undefined,
): ComposerDraft {
  const { replyId: _old, ...rest } = draft;
  return replyId === undefined ? rest : { ...rest, replyId };
}

export interface ComposerState {
  readonly draft: ComposerDraft;
  readonly sending: boolean;
  /** A send of this chat has an unknown outcome: the composer is locked. */
  readonly unknown: boolean;
  /** The conversation's consent is allowed. */
  readonly allowed: boolean;
  readonly error?: string;
}

/** Typing and sending are possible (`ComposerState.enabled`). */
export function composerEnabled(state: ComposerState): boolean {
  return (
    state.allowed &&
    !state.sending &&
    !state.unknown &&
    state.draft.requestId === undefined
  );
}

/** Synchronous draft storage (`composer-<sha256 of the conversation ID>`). */
export interface ComposerDraftStorage {
  /** The saved draft, or undefined. Throws when it cannot be read. */
  load(conversationId: string): ComposerDraft | undefined;
  save(conversationId: string, draft: ComposerDraft): void;
}

export interface ComposerPorts {
  readonly conversationId: string;
  readonly drafts: ComposerDraftStorage;
  readonly journal: SendJournal;
  readonly coordinator: SendCoordinator;
  readonly queries: Pick<ChatQueries, "queue">;
  /** False once the chat screen closed or its session ended. */
  readonly admit: () => boolean;
  /** Runs after a send was accepted (the timeline refreshes). */
  readonly accepted: () => void | Promise<void>;
  readonly newId: () => string;
  readonly now: () => number;
  /** Runs a send as session work (sign out waits for it). Default: runs it. */
  readonly run?: (work: () => Promise<void>) => void;
}

export class ChatComposer extends Store<ComposerState> {
  readonly #ports: ComposerPorts;
  #job: Promise<void> = Promise.resolve();

  constructor(ports: ComposerPorts) {
    super({
      draft: EMPTY_DRAFT,
      sending: false,
      unknown: false,
      allowed: false,
    });
    this.#ports = ports;
    this.reload();
  }

  get enabled(): boolean {
    return composerEnabled(this.getState());
  }

  /** Resolves when the send in flight settles. */
  wait(): Promise<void> {
    return this.#job;
  }

  /** The conversation's consent (the composer works only when allowed). */
  allow(allowed: boolean): void {
    this.#apply((state) =>
      state.allowed === allowed ? state : { ...state, allowed },
    );
  }

  /** Replaces the text and saves the draft. Ignored while disabled. */
  edit(text: string): void {
    if (!this.#ports.admit() || !this.enabled) return;
    const draft = editDraft(this.getState().draft, text);
    if (draft === this.getState().draft) return;
    this.setState((state) => ({ ...state, draft }));
    this.#persist();
  }

  /** Sets (or with undefined clears) the reply target and saves the draft. */
  reply(messageId: string | undefined): void {
    if (!this.#ports.admit() || !this.enabled) return;
    this.setState((state) => ({
      ...state,
      draft: withReply(state.draft, messageId),
    }));
    this.#persist();
  }

  /**
   * Re-reads the saved draft and the journal: applies an acceptance that
   * finished while the chat was closed, releases a reservation without a
   * journal entry, and locks on an unknown outcome.
   */
  reload(): void {
    if (!this.#ports.admit()) return;
    const { drafts, journal, conversationId: id } = this.#ports;
    try {
      let saved = drafts.load(id) ?? EMPTY_DRAFT;
      const references = journal
        .all()
        .filter(
          (entry) =>
            entry.conversationId === id && entry.deletionTargetId === undefined,
        );
      const accepted = references.find(
        (entry) => entry.id === saved.requestId && entry.phase === "accepted",
      );
      if (accepted?.messageId !== undefined) {
        saved = { ...saved, acceptedId: accepted.messageId };
        if (saved.revision === saved.submittedRevision) {
          saved = withReply(editDraft(saved, ""), undefined);
        }
        saved = releaseDraft(saved);
        drafts.save(id, saved);
      }
      const request = saved.requestId;
      if (
        !this.getState().sending &&
        request !== undefined &&
        !references.some((entry) => entry.id === request)
      ) {
        saved = releaseDraft(saved);
        drafts.save(id, saved);
      }
      const draft = saved;
      const unknown = composerOutcomeUnknown(references, id);
      this.setState((state) => {
        // The chat stays mounted while Settings clears the outcome (iOS
        // rebuilds it): drop the lock's message with the lock.
        if (!unknown && state.error === ComposerCopy.outcomeUnknown) {
          const { error: _error, ...rest } = state;
          return { ...rest, draft, unknown };
        }
        return { ...state, draft, unknown };
      });
    } catch {
      this.setState((state) => ({
        ...state,
        unknown: true,
        error: ComposerCopy.loadFailed,
      }));
    }
  }

  /** In-chat "Clear": removes this chat's unknown sends. Never resends. */
  clearReview(): void {
    if (!this.#ports.admit()) return;
    try {
      this.#ports.journal.clearUnknownSends(this.#ports.conversationId);
      this.setState((state) => ({
        ...state,
        draft: releaseDraft(state.draft),
        unknown: false,
      }));
      this.#persist();
    } catch {
      this.setState((state) => ({ ...state, error: ComposerCopy.clearFailed }));
    }
  }

  /** Sends the draft text (as a reply when a reply target is set). */
  submit(): void {
    const state = this.getState();
    if (
      !this.#ports.admit() ||
      !this.enabled ||
      state.draft.text.trim().length === 0
    ) {
      return;
    }
    const { drafts, conversationId: id } = this.#ports;
    const request = this.#ports.newId();
    const submitted = state.draft;
    const reserved: ComposerDraft = {
      ...submitted,
      requestId: request,
      submittedRevision: submitted.revision,
    };
    try {
      drafts.save(id, reserved);
    } catch {
      this.setState((current) => ({
        ...current,
        draft: submitted,
        error: ComposerCopy.saveFailed,
      }));
      return;
    }
    this.setState((current) => {
      const { error: _error, ...rest } = current;
      return { ...rest, draft: reserved, sending: true };
    });
    const work = this.#send(request, submitted);
    this.#job = work;
    const run = this.#ports.run ?? ((task) => void task());
    run(() => work);
  }

  async #send(request: string, submitted: ComposerDraft): Promise<void> {
    const { drafts, coordinator, queries, conversationId: id } = this.#ports;
    try {
      const content: OutboundContent =
        submitted.replyId === undefined
          ? { kind: "text", text: submitted.text }
          : {
              kind: "reply",
              parentId: submitted.replyId,
              text: submitted.text,
            };
      // A reopened composer may have released this reservation meanwhile.
      if (drafts.load(id)?.requestId !== request) {
        this.#apply((state) => ({ ...state, sending: false }));
        return;
      }
      const messageId = await coordinator.queue(
        {
          id: request,
          conversationId: id,
          phase: "queueing",
          createdAt: this.#ports.now(),
        },
        {
          admission: () => this.#ports.admit(),
          accept: () => queries.queue(id, content),
          acknowledge: (accepted) => {
            const current = this.#acknowledge(accepted, request, submitted);
            if (current !== undefined) {
              this.#apply((state) => ({ ...state, draft: current }));
            }
          },
        },
      );
      this.#apply((state) => {
        if (state.draft.requestId === undefined) {
          return { ...state, sending: false, unknown: false };
        }
        return {
          ...state,
          sending: false,
          unknown: false,
          draft: { ...state.draft, acceptedId: messageId },
          error: ComposerCopy.acceptedPending,
        };
      });
      await this.#ports.accepted();
    } catch {
      this.#fail(request);
    }
  }

  /**
   * Saves the accepted ID into the draft, then clears the text if it is
   * unedited since submission (and the reply if unchanged), then releases the
   * reservation. Returns the new draft, or undefined when the reservation
   * changed.
   */
  #acknowledge(
    messageId: string,
    request: string,
    submitted: ComposerDraft,
  ): ComposerDraft | undefined {
    const { drafts, conversationId: id } = this.#ports;
    let current = drafts.load(id) ?? EMPTY_DRAFT;
    if (
      current.requestId !== request ||
      current.submittedRevision !== submitted.revision
    ) {
      return undefined;
    }
    current = { ...current, acceptedId: messageId };
    drafts.save(id, current);
    if (current.revision === submitted.revision) {
      current = editDraft(current, "");
      if (current.replyId === submitted.replyId) {
        current = withReply(current, undefined);
      }
    }
    current = releaseDraft(current);
    drafts.save(id, current);
    return current;
  }

  #fail(request: string): void {
    const { drafts, journal, conversationId: id } = this.#ports;
    let unknown: boolean;
    try {
      unknown = journal
        .all()
        .some((entry) => entry.id === request && entry.phase === "queueing");
    } catch {
      unknown = true;
    }
    let released: ComposerDraft | undefined;
    if (!unknown) {
      try {
        const saved = drafts.load(id) ?? EMPTY_DRAFT;
        if (saved.requestId === request) {
          released = releaseDraft(saved);
          drafts.save(id, released);
        }
      } catch {
        this.#apply((state) => ({
          ...state,
          sending: false,
          error: ComposerCopy.saveFailed,
        }));
        return;
      }
    }
    this.#apply((state) => {
      const draft = released ?? state.draft;
      return {
        ...state,
        draft: unknown ? draft : releaseDraft(draft),
        sending: false,
        unknown,
        error: unknown ? ComposerCopy.outcomeUnknown : ComposerCopy.rejected,
      };
    });
  }

  #persist(): void {
    try {
      this.#ports.drafts.save(
        this.#ports.conversationId,
        this.getState().draft,
      );
    } catch {
      this.setState((state) => ({ ...state, error: ComposerCopy.saveFailed }));
    }
  }

  #apply(change: (state: ComposerState) => ComposerState): void {
    if (!this.#ports.admit()) return;
    this.setState(change);
  }
}
