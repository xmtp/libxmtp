// Settings › Send needs review (App/Features/Settings/SendReviewSection.swift):
// journal entries whose outcome is unknown, or deletions not yet confirmed.
// Clearing removes the entry (and releases the chat's composer); it never
// resends. Retry deletion publishes the accepted deletion operation again.

import { releaseDraft } from "@/core/chatComposer.ts";
import {
  type SendDraft,
  SendReviewCopy,
  needsReview,
} from "@/core/sendJournal.ts";
import { Store } from "@/core/store.ts";
import { chatServices } from "@/features/chat/chatServices";
import type { ActiveSession } from "@/session";

export interface SendReviewState {
  readonly drafts: readonly SendDraft[];
  /** Entry IDs whose deletion retry runs. */
  readonly retrying: ReadonlySet<string>;
  readonly error?: string;
}

export class SendReviewStore extends Store<SendReviewState> {
  readonly #session: ActiveSession;
  #disposed = false;

  constructor(session: ActiveSession) {
    super({ drafts: [], retrying: new Set() });
    this.#session = session;
    this.reload();
  }

  dispose(): void {
    this.#disposed = true;
  }

  #admits(): boolean {
    return !this.#disposed && this.#session.isCurrent();
  }

  /** Re-reads the journal (on appear). */
  reload(): void {
    if (!this.#admits()) return;
    try {
      const drafts = chatServices(this.#session)
        .journal.all()
        .filter(needsReview);
      this.setState((state) => ({ ...state, drafts }));
    } catch {
      this.setState((state) => ({
        ...state,
        error: SendReviewCopy.loadSendsFailed,
      }));
    }
  }

  /** "Retry deletion": publishes the accepted deletion operation again. */
  retryDeletion(draft: SendDraft): void {
    const messageId = draft.messageId;
    if (
      !this.#admits() ||
      messageId === undefined ||
      this.getState().retrying.has(draft.id)
    ) {
      return;
    }
    this.#setRetrying(draft.id, true);
    const services = chatServices(this.#session);
    services.run(async () => {
      let error: string | undefined;
      try {
        await services.coordinator.retry(draft.conversationId, messageId, () =>
          this.#admits(),
        );
      } catch {
        error = SendReviewCopy.deletionStillPending;
      }
      if (!this.#admits()) return;
      this.setState((state) => {
        const { error: _error, ...rest } = state;
        return error === undefined ? rest : { ...rest, error };
      });
      this.#setRetrying(draft.id, false);
      this.reload();
    });
  }

  /**
   * "Clear saved send status" / "Clear saved deletion status": removes the
   * entry; a send also releases its chat's composer reservation.
   */
  clear(draft: SendDraft): void {
    if (!this.#admits()) return;
    try {
      chatServices(this.#session).journal.remove(draft.id);
      if (draft.deletionTargetId === undefined) {
        const preferences = this.#session.preferences;
        const composer = preferences.composerDraft(draft.conversationId);
        if (composer !== undefined) {
          preferences.saveComposerDraft(
            draft.conversationId,
            releaseDraft(composer),
          );
        }
      }
      this.reload();
    } catch {
      this.setState((state) => ({
        ...state,
        error: SendReviewCopy.clearFailed,
      }));
    }
  }

  #setRetrying(id: string, on: boolean): void {
    this.setState((state) => {
      const retrying = new Set(state.retrying);
      if (on) retrying.add(id);
      else retrying.delete(id);
      return { ...state, retrying };
    });
  }
}
