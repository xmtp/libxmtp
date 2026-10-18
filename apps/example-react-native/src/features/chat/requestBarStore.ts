// The message request bar's decisions (ExampleCore/Conversations/RequestsStore.swift).
// Accept allows the conversation at once; Delete request and Block ask for
// confirmation first. Delete hides the request on this device; Block denies
// the conversation. Both then leave the chat for Chats.

import { Store } from "@/core/store.ts";
import type { ActiveSession } from "@/session";

export const RequestBarCopy = {
  deleteFailed: "Cannot delete request. Try again.",
  updateFailed: "Cannot update conversation. Try again.",
} as const;

export type RequestDecision = "delete" | "block";

export interface RequestBarState {
  /** The decision waiting for "Confirm request action". */
  readonly confirmation?: RequestDecision;
  readonly busy: boolean;
  readonly error?: string;
  /** Set once Delete request or Block finished: the screen returns to Chats. */
  readonly left: boolean;
}

export class RequestBarStore extends Store<RequestBarState> {
  readonly #session: ActiveSession;
  readonly #conversationId: string;
  readonly #admit: () => boolean;
  readonly #changed: () => Promise<void>;

  constructor(
    session: ActiveSession,
    conversationId: string,
    admit: () => boolean,
    changed: () => Promise<void>,
  ) {
    super({ busy: false, left: false });
    this.#session = session;
    this.#conversationId = conversationId;
    this.#admit = admit;
    this.#changed = changed;
  }

  accept(): void {
    void this.#decide("allowed");
  }

  requestDelete(): void {
    this.#confirm("delete");
  }

  requestBlock(): void {
    this.#confirm("block");
  }

  cancel(): void {
    if (!this.#admit()) return;
    this.setState((state) => {
      const { confirmation: _confirmation, ...rest } = state;
      return rest;
    });
  }

  /** Runs the confirmed decision. */
  confirm(): void {
    const decision = this.getState().confirmation;
    this.cancel();
    if (decision === "block") {
      void this.#decide("denied");
    } else if (decision === "delete") {
      try {
        this.#session.preferences.dismissRequest(this.#conversationId);
        this.setState((state) => {
          const { error: _error, ...rest } = state;
          return { ...rest, left: true };
        });
      } catch {
        this.setState((state) => ({
          ...state,
          error: RequestBarCopy.deleteFailed,
        }));
      }
    }
  }

  #confirm(decision: RequestDecision): void {
    if (!this.#admit()) return;
    this.setState((state) => {
      const { error: _error, ...rest } = state;
      return { ...rest, confirmation: decision };
    });
  }

  async #decide(consent: "allowed" | "denied"): Promise<void> {
    if (!this.#admit() || this.getState().busy) return;
    this.setState((state) => {
      const { error: _error, ...rest } = state;
      return { ...rest, busy: true };
    });
    try {
      const conversation = await this.#session.client.conversations.getById(
        this.#conversationId,
      );
      if (conversation === undefined) throw new Error("No conversation.");
      await conversation.updateConsentState(consent);
      if (!this.#admit()) return;
      this.setState((state) => ({
        ...state,
        busy: false,
        left: consent === "denied",
      }));
      await this.#changed();
    } catch (error) {
      console.warn("Cannot update the request", error);
      if (!this.#admit()) return;
      this.setState((state) => ({
        ...state,
        busy: false,
        error: RequestBarCopy.updateFailed,
      }));
    }
  }
}
