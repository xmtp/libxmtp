// Message actions: react, delete, retry.
//
// Reactions and deletions go through the send journal like texts. A deletion
// journals its target before the SDK call and publishes the deletion's
// operation ID; one whose outcome is unknown stays in Settings › Send needs
// review. Retry publishes the same message ID again.

import type { ChatQueries } from "./chatHistory.ts";
import { type ChatMessage, hasOwnReaction, isDeleted } from "./chatMessages.ts";
import { type SendCoordinator, SendError } from "./sendJournal.ts";
import { Store } from "./store.ts";

export const MessageActionCopy = {
  failed: "Cannot complete this message action. Try again.",
  deletionNeedsReview: "Deletion needs review in Settings.",
} as const;

export interface MessageActionState {
  /** The message loaded with `load` (the Reactions sheet and emoji picker). */
  readonly message?: ChatMessage;
  /** The message waiting for the "Delete message?" confirmation. */
  readonly deleteTarget?: ChatMessage;
  readonly error?: string;
  readonly busy: boolean;
}

export interface MessageActionPorts {
  readonly queries: Pick<
    ChatQueries,
    "message" | "conversation" | "queue" | "delete"
  >;
  readonly coordinator: SendCoordinator;
  /** The own inbox ID. */
  readonly own: string;
  /** False once the screen closed or its session ended. */
  readonly admit: () => boolean;
  /** Runs after an action changed the conversation (the timeline refreshes). */
  readonly changed: () => void | Promise<void>;
  readonly newId: () => string;
  readonly now: () => number;
  /** Runs an action as session work. Default: runs it. */
  readonly run?: (work: () => Promise<void>) => void;
}

/** Whether `row` can be deleted: own, published, not deleted, consent allowed. */
export function canDelete(
  row: ChatMessage,
  own: string,
  allowed: boolean,
): boolean {
  return (
    allowed &&
    row.sender === own &&
    row.delivery === "published" &&
    !isDeleted(row.content)
  );
}

export class MessageActions extends Store<MessageActionState> {
  readonly #ports: MessageActionPorts;
  #allowed = false;
  readonly #jobs = new Set<Promise<void>>();

  constructor(ports: MessageActionPorts) {
    super({ busy: false });
    this.#ports = ports;
  }

  /** The conversation's consent is allowed (actions other than Copy need it). */
  get allowed(): boolean {
    return this.#allowed;
  }

  set allowed(value: boolean) {
    this.#allowed = value;
  }

  /** Resolves when every action in flight settled. */
  async wait(): Promise<void> {
    await Promise.all(Array.from(this.#jobs));
  }

  /** Loads one message (and its conversation's consent) into `message`. */
  load(messageId: string): void {
    if (!this.#ports.admit()) return;
    this.setState((state) => ({ ...state, busy: true }));
    this.#start(async () => {
      const row = await this.#ports.queries.message(messageId);
      if (row !== undefined) {
        const conversation = await this.#ports.queries.conversation(
          row.conversationId,
        );
        if (!this.#ports.admit()) return;
        this.#allowed = conversation.consent === "allowed";
      }
      this.#apply((state) => {
        const { message: _message, error: _error, ...rest } = state;
        return {
          ...rest,
          busy: false,
          ...(row === undefined ? {} : { message: row }),
        };
      });
    });
  }

  /** Toggles the own reaction `emoji` on `row`: removes it when present, else adds it. */
  react(row: ChatMessage, emoji: string): void {
    if (!this.#ports.admit() || !this.#allowed) return;
    this.setState((state) => ({ ...state, busy: true }));
    this.#start(async () => {
      const remove = hasOwnReaction(row, emoji, this.#ports.own);
      await this.#ports.coordinator.queue(
        {
          id: this.#ports.newId(),
          conversationId: row.conversationId,
          phase: "queueing",
          createdAt: this.#ports.now(),
        },
        {
          admission: () => this.#ports.admit() && this.#allowed,
          accept: () =>
            this.#ports.queries.queue(row.conversationId, {
              kind: "reaction",
              parentId: row.id,
              emoji,
              remove,
            }),
        },
      );
      const refreshed = await this.#ports.queries.message(row.id);
      this.#succeeded(refreshed);
      await this.#ports.changed();
    });
  }

  /** Asks for the "Delete message?" confirmation (own published rows only). */
  requestDelete(row: ChatMessage): void {
    if (!this.#ports.admit()) return;
    if (!canDelete(row, this.#ports.own, this.#allowed)) return;
    this.setState((state) => ({ ...state, deleteTarget: row }));
  }

  cancelDelete(): void {
    if (!this.#ports.admit()) return;
    this.setState((state) => {
      const { deleteTarget: _target, ...rest } = state;
      return rest;
    });
  }

  /**
   * Deletes `row` through the journal (no confirmation; callers confirm
   * first). A failure with the deletion journaled shows "Deletion needs
   * review in Settings."
   */
  deleteMessage(row: ChatMessage): void {
    if (!this.#ports.admit()) return;
    if (!canDelete(row, this.#ports.own, this.#allowed)) return;
    this.setState((state) => ({ ...state, busy: true }));
    const { coordinator } = this.#ports;
    const job = (async () => {
      let failed = false;
      try {
        await coordinator.delete(row.conversationId, row.id, {
          admission: () => this.#ports.admit() && this.#allowed,
          deleteMessage: () => this.#ports.queries.delete(row.id),
          newId: this.#ports.newId,
          now: this.#ports.now,
        });
      } catch (error) {
        console.warn("Cannot delete the message", error);
        failed = true;
      }
      const needsReview = this.#deletionNeedsReview(row.id);
      const error = needsReview
        ? MessageActionCopy.deletionNeedsReview
        : failed
          ? MessageActionCopy.failed
          : undefined;
      this.#apply((state) => {
        const { error: _error, ...rest } = state;
        return {
          ...rest,
          busy: false,
          ...(error === undefined ? {} : { error }),
        };
      });
      if (!failed || needsReview) await this.#ports.changed();
    })();
    this.#track(job);
  }

  /** Publishes an own unpublished message again (the same ID). */
  retry(row: ChatMessage): void {
    if (!this.#ports.admit() || !this.#allowed) return;
    this.setState((state) => ({ ...state, busy: true }));
    this.#start(async () => {
      if (row.sender !== this.#ports.own) throw new SendError("rejected");
      await this.#ports.coordinator.retry(
        row.conversationId,
        row.id,
        () => this.#ports.admit() && this.#allowed,
      );
      this.#succeeded(undefined);
      await this.#ports.changed();
    });
  }

  #succeeded(message: ChatMessage | undefined): void {
    this.#apply((state) => {
      const { error: _error, ...rest } = state;
      return {
        ...rest,
        busy: false,
        ...(message === undefined ? {} : { message }),
      };
    });
  }

  #deletionNeedsReview(messageId: string): boolean {
    try {
      return this.#ports.coordinator.deletionNeedsReview(messageId);
    } catch {
      return true;
    }
  }

  #start(work: () => Promise<void>): void {
    const job = (async () => {
      try {
        await work();
      } catch (error) {
        console.warn("Message action failed", error);
        this.#apply((state) => ({
          ...state,
          busy: false,
          error: MessageActionCopy.failed,
        }));
      }
    })();
    this.#track(job);
  }

  #track(job: Promise<void>): void {
    this.#jobs.add(job);
    void job.finally(() => this.#jobs.delete(job));
    const run = this.#ports.run;
    if (run !== undefined) run(() => job);
  }

  #apply(change: (state: MessageActionState) => MessageActionState): void {
    if (!this.#ports.admit()) return;
    this.setState(change);
  }
}
