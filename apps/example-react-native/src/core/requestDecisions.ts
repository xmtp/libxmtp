// Accept, delete and block decisions on conversations
// (ExampleCore/Conversations/RequestsStore.swift).
//
// - Accept runs at once and opens the chat with Chats below it.
// - Delete and Block ask first. Delete is local only: the request joins the
//   dismissed list on this device. Block sets the consent to denied. Both
//   return to Chats.
// - One consent write runs at a time.

import { Store } from "./store.ts";

export type RequestDecision =
  | { readonly kind: "delete"; readonly id: string }
  | { readonly kind: "block"; readonly id: string };

export interface RequestDecisionsState {
  /** The decision waiting for confirmation. */
  readonly confirmation?: RequestDecision;
  readonly busy: boolean;
  readonly error?: string;
}

/** Where to go after a decision (`RouteOwner.setPath` / `replace(.chats)`). */
export type DecisionOutcome =
  | { readonly kind: "openChat"; readonly id: string }
  | { readonly kind: "chats" };

export interface RequestDecisionsOptions {
  setConsent(id: string, consent: "allowed" | "denied"): Promise<void>;
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
  #inFlight: symbol | undefined;
  #disposed = false;

  constructor(options: RequestDecisionsOptions) {
    super({ busy: false });
    this.#options = options;
  }

  accept(id: string): void {
    if (!this.#begin()) return;
    this.#decide(id, "allowed");
  }

  requestDelete(id: string): void {
    if (!this.#begin()) return;
    this.setState((state) => ({
      ...state,
      error: undefined,
      confirmation: { kind: "delete", id },
    }));
  }

  requestBlock(id: string): void {
    if (!this.#begin()) return;
    this.setState((state) => ({
      ...state,
      error: undefined,
      confirmation: { kind: "block", id },
    }));
  }

  cancel(): void {
    if (!this.#begin()) return;
    this.setState((state) => ({ ...state, confirmation: undefined }));
  }

  /**
   * Runs the decision waiting for confirmation. A dialog passes the decision
   * it showed, because its dismissal may arrive before its action.
   */
  confirm(
    decision: RequestDecision | undefined = this.getState().confirmation,
  ): void {
    if (!this.#begin()) return;
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

  /** Admits an action; clears a finished operation's busy flag. */
  #begin(): boolean {
    if (!this.#open()) return false;
    if (this.#inFlight === undefined && this.getState().busy) {
      this.setState((state) => ({ ...state, busy: false }));
    }
    return true;
  }

  #decide(id: string, consent: "allowed" | "denied"): void {
    if (this.#inFlight !== undefined) return;
    const operation = Symbol("decision");
    this.#inFlight = operation;
    this.setState((state) => ({ ...state, busy: true, error: undefined }));
    void this.#options
      .setConsent(id, consent)
      .then(
        () => {
          if (!this.#open()) return;
          this.setState((state) => ({
            ...state,
            busy: false,
            error: undefined,
          }));
          this.#options.onOutcome(
            consent === "allowed"
              ? { kind: "openChat", id }
              : { kind: "chats" },
          );
        },
        () => {
          if (!this.#open()) return;
          this.setState((state) => ({
            ...state,
            busy: false,
            error: DecisionCopy.updateFailed,
          }));
        },
      )
      .finally(() => {
        if (this.#inFlight === operation) this.#inFlight = undefined;
      });
  }
}
