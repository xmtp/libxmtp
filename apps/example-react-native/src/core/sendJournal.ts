// The send and delete journal (ExampleCore/Send/SendDraftStore.swift,
// SendCoordinator.swift, AcceptedMessageCommit.swift).
//
// A send saves a `queueing` entry before any SDK call, then `accepted` with the
// message ID before the composer clears, then removes the entry once the
// message is published. A failure that is not a known rejection leaves the
// `queueing` entry: the outcome is unknown, so the composer locks and
// Settings › Send needs review lists it. Retry publishes the same message ID.

export type SendPhase = "queueing" | "accepted";

export interface SendDraft {
  /** The client request ID (a UUID), saved before the SDK call. */
  readonly id: string;
  readonly conversationId: string;
  readonly phase: SendPhase;
  /** The SDK message ID once the send (or the deletion) was accepted. */
  readonly messageId?: string;
  /** For a deletion: the message being deleted. */
  readonly deletionTargetId?: string;
  /** Milliseconds since the epoch. */
  readonly createdAt: number;
}

/** Synchronous storage for the journal (the `send-drafts` preference). */
export interface SendJournalStorage {
  read(): readonly SendDraft[];
  write(drafts: readonly SendDraft[]): void;
}

/** Why a send stopped. Known rejections restore the journal; others leave it. */
export type SendFailure =
  | "rejected"
  | "unavailable"
  | "stale"
  | "duplicate"
  | "storage";

export class SendError extends Error {
  readonly kind: SendFailure;

  constructor(kind: SendFailure, message = `Send failed: ${kind}`) {
    super(message);
    this.name = "SendError";
    this.kind = kind;
  }
}

/** A failure that proves the SDK did not queue the message. */
export function isKnownRejection(error: unknown): boolean {
  return (
    error instanceof SendError &&
    (error.kind === "rejected" ||
      error.kind === "unavailable" ||
      error.kind === "stale")
  );
}

export const SendReviewCopy = {
  empty: "No sends need review",
  sendTitle: "Send outcome unknown",
  deletionPending: "Deletion pending",
  deletionUnknown: "Deletion outcome unknown",
  sendDetail: "The original message may already exist.",
  deletionDetail: "Check this chat before trying to delete again.",
  viewChat: "View chat",
  retryDeletion: "Retry deletion",
  clearSend: "Clear saved send status",
  clearDeletion: "Clear saved deletion status",
  explanation:
    "Clear the saved send status after you check the conversation. This does not delete or cancel the original message.",
  deletionExplanation: "Clearing this status does not undo the deletion.",
  loadSendsFailed: "Cannot load saved send status.",
  loadDeletionsFailed: "Cannot load saved deletion status.",
  clearFailed: "Cannot clear saved send status.",
  deletionStillPending: "Deletion is still pending.",
} as const;

/** Whether Settings › Send needs review lists the entry. */
export function needsReview(draft: SendDraft): boolean {
  return (
    draft.deletionTargetId !== undefined ||
    (draft.phase === "queueing" && draft.messageId === undefined)
  );
}

/** The review row title. */
export function reviewTitle(draft: SendDraft): string {
  if (draft.deletionTargetId === undefined) return SendReviewCopy.sendTitle;
  return draft.phase === "accepted"
    ? SendReviewCopy.deletionPending
    : SendReviewCopy.deletionUnknown;
}

/** The review row detail. */
export function reviewDetail(draft: SendDraft): string {
  return draft.deletionTargetId === undefined
    ? SendReviewCopy.sendDetail
    : SendReviewCopy.deletionDetail;
}

/** "Retry deletion" shows only for an accepted deletion with its operation ID. */
export function canRetryDeletion(draft: SendDraft): boolean {
  return (
    draft.deletionTargetId !== undefined &&
    draft.phase === "accepted" &&
    draft.messageId !== undefined
  );
}

/**
 * The composer of `conversationId` is locked as "Send outcome unknown" when a
 * send of that conversation is still `queueing` without a message ID.
 */
export function composerOutcomeUnknown(
  drafts: readonly SendDraft[],
  conversationId: string,
): boolean {
  return drafts.some(
    (draft) =>
      draft.conversationId === conversationId &&
      draft.deletionTargetId === undefined &&
      draft.phase === "queueing" &&
      draft.messageId === undefined,
  );
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Reads a stored journal. Throws on malformed data (the caller shows a load error). */
export function parseSendDrafts(value: unknown): readonly SendDraft[] {
  if (!Array.isArray(value)) throw new Error("The send journal is not a list.");
  return value.map((entry): SendDraft => {
    if (
      !isRecord(entry) ||
      typeof entry["id"] !== "string" ||
      typeof entry["conversationId"] !== "string" ||
      (entry["phase"] !== "queueing" && entry["phase"] !== "accepted") ||
      typeof entry["createdAt"] !== "number"
    ) {
      throw new Error("A send journal entry is malformed.");
    }
    const messageId = entry["messageId"];
    const deletionTargetId = entry["deletionTargetId"];
    if (
      (messageId !== undefined && typeof messageId !== "string") ||
      (deletionTargetId !== undefined && typeof deletionTargetId !== "string")
    ) {
      throw new Error("A send journal entry is malformed.");
    }
    return {
      id: entry["id"],
      conversationId: entry["conversationId"],
      phase: entry["phase"],
      createdAt: entry["createdAt"],
      ...(messageId === undefined ? {} : { messageId }),
      ...(deletionTargetId === undefined ? {} : { deletionTargetId }),
    };
  });
}

/** The journal entries, read and written through `storage` on every call. */
export class SendJournal {
  readonly #storage: SendJournalStorage;

  constructor(storage: SendJournalStorage) {
    this.#storage = storage;
  }

  all(): readonly SendDraft[] {
    return this.#storage.read();
  }

  /** Replaces the entry with the same ID (or appends it). */
  save(draft: SendDraft): void {
    this.#storage.write([
      ...this.#storage.read().filter((entry) => entry.id !== draft.id),
      draft,
    ]);
  }

  remove(id: string): void {
    this.#storage.write(
      this.#storage.read().filter((entry) => entry.id !== id),
    );
  }

  /** In-chat "Clear": removes this conversation's unknown sends. Never resends. */
  clearUnknownSends(conversationId: string): void {
    this.#storage.write(
      this.#storage
        .read()
        .filter(
          (entry) =>
            !(
              entry.conversationId === conversationId &&
              entry.phase === "queueing" &&
              entry.deletionTargetId === undefined
            ),
        ),
    );
  }
}

/** The SDK reads the coordinator needs (the chat lane maps SDK errors to `SendError`). */
export interface SendQueries {
  /** The conversation's consent state now. */
  consent(conversationId: string): Promise<"allowed" | "unknown" | "denied">;
  /** The message, if it exists: its conversation and whether it is published. */
  message(
    messageId: string,
  ): Promise<
    { readonly conversationId: string; readonly published: boolean } | undefined
  >;
  /** `conversation.publishMessage(messageId)`. */
  publish(conversationId: string, messageId: string): Promise<void>;
}

export interface QueueOptions {
  /** False once the chat screen that started the send is gone. */
  readonly admission: () => boolean;
  /** Queues locally (`sendText(…, { optimistic: true })`) and returns the message ID. */
  readonly accept: () => Promise<string>;
  /** Runs after the accepted ID is saved: clears the composer if unedited. */
  readonly acknowledge?: (messageId: string) => void;
  /** Runs after the consent check, before the SDK call (for example, a draft save). */
  readonly beforeQueue?: () => void;
}

interface PendingCommit {
  readonly messageId: string;
  finish(): void;
}

/** Runs sends and deletions through the journal. */
export class SendCoordinator {
  readonly #journal: SendJournal;
  readonly #queries: SendQueries;
  readonly #accepts: () => boolean;
  readonly #active = new Set<string>();
  readonly #pending = new Map<string, PendingCommit>();

  /** `accepts` is false once the session ended. */
  constructor(
    journal: SendJournal,
    queries: SendQueries,
    accepts: () => boolean,
  ) {
    this.#journal = journal;
    this.#queries = queries;
    this.#accepts = accepts;
  }

  /**
   * Queues a send (or a deletion) described by `draft`, saving `queueing`
   * before the SDK call and `accepted` with the ID before `acknowledge`, then
   * publishes. A publication failure leaves the message for Retry. Returns
   * the message ID.
   */
  async queue(draft: SendDraft, options: QueueOptions): Promise<string> {
    if (!this.#accepts() || !options.admission()) throw new SendError("stale");
    if (this.#active.has(draft.id)) throw new SendError("duplicate");
    this.#active.add(draft.id);
    try {
      const previous = this.#journal
        .all()
        .find((entry) => entry.id === draft.id);
      this.#journal.save(draft);
      let consent: string;
      try {
        consent = await this.#queries.consent(draft.conversationId);
      } catch (error) {
        this.#restore(previous, draft);
        throw error;
      }
      if (!this.#accepts() || !options.admission() || consent !== "allowed") {
        this.#restore(previous, draft);
        throw new SendError("stale");
      }
      try {
        options.beforeQueue?.();
      } catch (error) {
        this.#restore(previous, draft);
        throw error;
      }
      let messageId: string;
      try {
        messageId = await options.accept();
      } catch (error) {
        // Anything else leaves `queueing`: the outcome is unknown.
        if (isKnownRejection(error)) this.#restore(previous, draft);
        throw error;
      }
      const accepted: SendDraft = { ...draft, phase: "accepted", messageId };
      const commit: PendingCommit = {
        messageId,
        finish: () => {
          this.#journal.save(accepted);
          options.acknowledge?.(messageId);
        },
      };
      this.#pending.set(draft.id, commit);
      try {
        commit.finish();
        this.#pending.delete(draft.id);
      } catch {
        // Kept in memory; `recoverAccepted` replays it.
        return messageId;
      }
      if (!this.#accepts() || !options.admission()) return messageId;
      try {
        await this.retry(draft.conversationId, messageId, options.admission);
      } catch {
        // Accepted and durable: Retry publishes the same ID later.
      }
      return messageId;
    } finally {
      this.#active.delete(draft.id);
    }
  }

  /** Replays acceptance commits whose durable write failed. */
  recoverAccepted(): void {
    for (const [id, commit] of Array.from(this.#pending)) {
      commit.finish();
      this.#pending.delete(id);
    }
  }

  /**
   * Publishes an accepted message again (the same ID), then removes its
   * journal entries. Requires the message in this conversation and consent.
   */
  async retry(
    conversationId: string,
    messageId: string,
    admission: () => boolean = () => true,
  ): Promise<void> {
    if (!this.#accepts() || !admission()) throw new SendError("stale");
    for (const [id, commit] of Array.from(this.#pending)) {
      if (commit.messageId !== messageId) continue;
      commit.finish();
      this.#pending.delete(id);
    }
    const message = await this.#queries.message(messageId);
    if (message?.conversationId !== conversationId) {
      throw new SendError("unavailable");
    }
    const consent = await this.#queries.consent(conversationId);
    if (!this.#accepts() || !admission() || consent !== "allowed") {
      throw new SendError("stale");
    }
    if (!message.published) {
      await this.#queries.publish(conversationId, messageId);
    }
    if (!this.#accepts()) return;
    for (const draft of this.#journal.all()) {
      if (draft.messageId === messageId) this.#journal.remove(draft.id);
    }
  }

  /**
   * Deletes a message through the journal. A deletion already accepted for
   * the message is retried instead; one with an unknown outcome refuses.
   */
  async delete(
    conversationId: string,
    targetId: string,
    options: {
      readonly admission: () => boolean;
      readonly deleteMessage: () => Promise<string>;
      readonly newId: () => string;
      readonly now: () => number;
    },
  ): Promise<string> {
    if (!this.#accepts() || !options.admission()) throw new SendError("stale");
    this.recoverAccepted();
    const saved = this.#journal
      .all()
      .find((entry) => entry.deletionTargetId === targetId);
    if (saved !== undefined) {
      if (
        saved.conversationId !== conversationId ||
        saved.phase !== "accepted" ||
        saved.messageId === undefined
      ) {
        throw new SendError("duplicate");
      }
      try {
        await this.retry(conversationId, saved.messageId, options.admission);
      } catch {
        // Still pending; Settings › Send needs review offers Retry deletion.
      }
      return saved.messageId;
    }
    return this.queue(
      {
        id: options.newId(),
        conversationId,
        phase: "queueing",
        deletionTargetId: targetId,
        createdAt: options.now(),
      },
      { admission: options.admission, accept: options.deleteMessage },
    );
  }

  /** True while a deletion of `messageId` is journaled. */
  deletionNeedsReview(messageId: string): boolean {
    return this.#journal
      .all()
      .some((entry) => entry.deletionTargetId === messageId);
  }

  /** The accepted message ID of a request, if its send was accepted. */
  acceptedMessageId(
    draftId: string,
    conversationId: string,
  ): string | undefined {
    return this.#journal
      .all()
      .find(
        (entry) =>
          entry.id === draftId &&
          entry.conversationId === conversationId &&
          entry.phase === "accepted",
      )?.messageId;
  }

  #restore(previous: SendDraft | undefined, draft: SendDraft): void {
    if (previous === undefined) this.#journal.remove(draft.id);
    else this.#journal.save(previous);
  }
}
