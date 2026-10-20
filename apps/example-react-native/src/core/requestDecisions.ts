// Accept, delete and block decisions on conversations.
//
// - Accept runs at once and opens the chat with Chats below it.
// - Delete and Block ask first. Delete is local only: the request joins the
//   dismissed list on this device. Block sets the consent to denied. Both
//   return to Chats.
// - Consent writes run one at a time, in order: a decision made while one
//   runs waits for it, so no confirmed decision is dropped.

import { Store } from "./store.ts";

type Consent = "allowed" | "denied";

export type RequestDecision =
  | { readonly kind: "delete"; readonly id: string }
  | { readonly kind: "block"; readonly id: string };

export interface RequestDecisionsState {
  /** The decision waiting for confirmation. */
  readonly confirmation?: RequestDecision;
  readonly busy: boolean;
  readonly error?: string;
}

/** Where to go after a decision: the chat, or back to Chats. */
export type DecisionOutcome =
  | { readonly kind: "openChat"; readonly id: string }
  | { readonly kind: "chats" };

export interface RequestDecisionsOptions {
  setConsent(id: string, consent: Consent): Promise<void>;
  /** Hides a request on this device (`dismissRequest`). Throws when it cannot save. */
  dismissRequest(id: string): void;
  /** False once the session that created the store ended. */
  readonly isCurrent: () => boolean;
  /** Called after a decision succeeds, while the store is open. */
  readonly onOutcome: (outcome: DecisionOutcome) => void;
}

export const DecisionCopy = {
  updateFailed: "Cannot update conversation. Try again.",
  deleteFailed: "Cannot delete request. Try again.",
} as const;

export class RequestDecisionsStore extends Store<RequestDecisionsState> {
  readonly #options: RequestDecisionsOptions;
  /** Consent writes waiting for the running one. */
  readonly #queue: { readonly id: string; readonly consent: Consent }[] = [];
  #running = false;
  #disposed = false;

  constructor(options: RequestDecisionsOptions) {
    super({ busy: false });
    this.#options = options;
  }

  accept(id: string): void {
    if (!this.#open()) return;
    this.#decide(id, "allowed");
  }

  requestDelete(id: string): void {
    if (!this.#open()) return;
    this.setState((state) => ({
      ...state,
      error: undefined,
      confirmation: { kind: "delete", id },
    }));
  }

  requestBlock(id: string): void {
    if (!this.#open()) return;
    this.setState((state) => ({
      ...state,
      error: undefined,
      confirmation: { kind: "block", id },
    }));
  }

  cancel(): void {
    if (!this.#open()) return;
    this.setState((state) => ({ ...state, confirmation: undefined }));
  }

  /**
   * Runs the decision waiting for confirmation. A dialog passes the decision
   * it showed, because its dismissal may arrive before its action.
   */
  confirm(decision: RequestDecision | undefined): void {
    if (!this.#open()) return;
    if (decision === undefined) return;
    this.setState((state) => ({ ...state, confirmation: undefined }));
    if (decision.kind === "block") {
      this.#decide(decision.id, "denied");
      return;
    }
    try {
      this.#options.dismissRequest(decision.id);
    } catch {
      this.setState((state) => ({
        ...state,
        error: DecisionCopy.deleteFailed,
      }));
      return;
    }
    this.setState((state) => ({ ...state, error: undefined }));
    this.#options.onOutcome({ kind: "chats" });
  }

  /** True after `dispose`; a closed store never changes again. */
  get disposed(): boolean {
    return this.#disposed;
  }

  dispose(): void {
    this.#disposed = true;
  }

  #open(): boolean {
    return !this.#disposed && this.#options.isCurrent();
  }

  #decide(id: string, consent: Consent): void {
    this.#queue.push({ id, consent });
    this.setState((state) => ({ ...state, busy: true, error: undefined }));
    if (!this.#running) void this.#drain();
  }

  /**
   * Runs the queued writes in order. A write runs after the store closed
   * (the first decision's navigation may close the screen), but not after
   * the session ended; results reach the state only while the store is open.
   */
  async #drain(): Promise<void> {
    this.#running = true;
    for (
      let next = this.#queue.shift();
      next !== undefined && this.#options.isCurrent();
      next = this.#queue.shift()
    ) {
      const { id, consent } = next;
      try {
        await this.#options.setConsent(id, consent);
      } catch {
        if (this.#open()) {
          this.setState((state) => ({
            ...state,
            error: DecisionCopy.updateFailed,
          }));
        }
        continue;
      }
      if (this.#open()) {
        this.#options.onOutcome(
          consent === "allowed" ? { kind: "openChat", id } : { kind: "chats" },
        );
      }
    }
    this.#queue.length = 0;
    this.#running = false;
    if (this.#open()) this.setState((state) => ({ ...state, busy: false }));
  }
}
