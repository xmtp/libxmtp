// Upload drafts and file downloads of one screen.
//
// Staging hands a copied file to the SDK (`attachments.create`), which
// encrypts it, and saves a draft. Send uploads first, then queues the message
// through the send journal (journal ID = draft ID), then removes the draft once
// the message is published. An ambiguous queue failure leaves the draft
// `unknown` ("Review send"); a definite rejection restores it for another send
// without uploading again. Downloads need a tap, re-check consent first, run
// once per message, and are all dropped when consent changes.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import {
  type AttachmentDraftStore,
  AttachmentError,
  type FileCard,
  type FileDraft,
  type FilePhase,
  type VerifiedFile,
  VerifiedFileCache,
  AttachmentCopy,
  defaultCard,
  mimeTypeFor,
  sameDraft,
  sameFile,
} from "./attachments.ts";
import {
  type SendCoordinator,
  SendError,
  isKnownRejection,
} from "./sendJournal.ts";
import { Store } from "./store.ts";

/** The SDK calls behind attachments. */
export interface AttachmentGateway {
  /** The server's upload limit, or undefined when it offers no attachments. */
  capability(): bigint | undefined;
  /** Encrypts a staged file into the SDK's store (`attachments.create`). */
  create(
    path: string,
    filename: string,
    mimeType: string,
  ): Promise<RemoteAttachment>;
  /** Uploads the SDK still holds (`attachments.listPending`). */
  pending(): Promise<RemoteAttachment[]>;
  /** Uploads and returns the final status phase. Throws `AttachmentError`. */
  upload(file: RemoteAttachment): Promise<FilePhase>;
  /** Downloads, checks the digest and decrypts. Throws `AttachmentError`. */
  download(file: RemoteAttachment): Promise<VerifiedFile>;
  /** Deletes the SDK's local copy and record (`attachments.deleteLocal`). */
  discard(file: RemoteAttachment): Promise<void>;
}

export interface AttachmentStoreState {
  /** The upload limit; undefined hides the attach menu. */
  readonly limit?: bigint;
  /** True after the first load finished. */
  readonly loaded: boolean;
  /** Every saved draft (the chat shows its own; recovery shows all). */
  readonly drafts: readonly FileDraft[];
  /** Download state by message ID. */
  readonly cards: Readonly<Record<string, FileCard>>;
  readonly error?: string;
}

export interface AttachmentStorePorts {
  readonly gateway: AttachmentGateway;
  readonly drafts: AttachmentDraftStore;
  readonly coordinator: SendCoordinator;
  /** The chat whose drafts this store stages; undefined on recovery screens. */
  readonly conversationId?: string;
  /** False once the screen is gone or the session ended. */
  readonly admit: () => boolean;
  /** False once the session ended (cleanup still runs after the screen left). */
  readonly acceptsSession: () => boolean;
  /** Runs work as session work: sign-out waits for it before ending the client. */
  readonly run: (job: () => Promise<void>) => void;
  /** The conversation's consent is allowed (sends and downloads). */
  readonly allowed: () => boolean;
  /** Re-reads consent right before a download. */
  readonly authorizeDownload?: () => Promise<boolean>;
  /** Queues `sendRemoteAttachment(file, { optimistic: true })`; returns the message ID. */
  readonly sendFile: (
    conversationId: string,
    file: RemoteAttachment,
  ) => Promise<string>;
  /** True when the message is published in that conversation. */
  readonly published: (
    conversationId: string,
    messageId: string,
  ) => Promise<boolean>;
  /**
   * Draft IDs whose send runs, shared by every store of the session: a load
   * leaves their upload alone, and no other store sends them meanwhile.
   */
  readonly activeSends: Set<string>;
  /** Removes a staged copy (after `create`, whether it worked or not). */
  readonly removeStaged: (path: string) => void;
  /** Runs after a draft's message was published (the chat refreshes). */
  readonly changed?: () => Promise<void>;
  readonly newId: () => string;
  readonly now: () => number;
}

type Failed =
  | { readonly kind: "send"; readonly id: string }
  | {
      readonly kind: "download";
      readonly messageId: string;
      readonly file: RemoteAttachment;
    }
  | { readonly kind: "other" };

export class AttachmentStore extends Store<AttachmentStoreState> {
  readonly #ports: AttachmentStorePorts;
  readonly #verified = new VerifiedFileCache();
  #revision = 0;
  readonly #admissions = new Map<string, object>();
  readonly #discarded = new Set<string>();
  readonly #discardedFiles = new Map<string, RemoteAttachment>();
  readonly #generations = new Map<string, number>();

  constructor(ports: AttachmentStorePorts) {
    super({ loaded: false, drafts: [], cards: {} });
    this.#ports = ports;
  }

  /** A message's card: its download state, or "available". */
  card(messageId: string, file: RemoteAttachment): FileCard {
    return this.getState().cards[messageId] ?? defaultCard(messageId, file);
  }

  /** The verified file of a message, while it is cached. */
  verified(messageId: string): VerifiedFile | undefined {
    return this.#verified.get(messageId);
  }

  #apply(change: (state: AttachmentStoreState) => AttachmentStoreState) {
    if (this.#ports.admit()) this.setState(change);
  }

  /** Starts `work` as session work, so sign-out waits for its SDK calls. */
  #session(work: () => Promise<void>): Promise<void> {
    const job = work();
    this.#ports.run(() => job);
    return job;
  }

  #canSend(id: string, generation: number): boolean {
    if (
      !this.#ports.admit() ||
      !this.#ports.allowed() ||
      this.#discarded.has(id) ||
      (this.#generations.get(id) ?? 0) !== generation
    ) {
      return false;
    }
    try {
      return this.#ports.drafts.all().some((draft) => draft.id === id);
    } catch {
      return false;
    }
  }

  /**
   * Re-reads the saved drafts only: no SDK import and no reset of interrupted
   * uploads, so a running send keeps its phase. A chat calls it when it
   * attaches again, as drafts may have changed while it was detached.
   */
  reloadDrafts(): void {
    try {
      this.#reloadDrafts();
    } catch (error) {
      this.#handleFailure(error, { kind: "other" });
    }
  }

  #reloadDrafts(): void {
    const drafts = this.#ports.drafts.all();
    this.#apply((state) => ({ ...state, drafts }));
  }

  /**
   * Shows each draft any store of the session saves or removes, until the
   * returned function runs. A screen watches while it shows drafts: another
   * screen's send can finish, stop or remove a draft meanwhile.
   */
  watchDrafts(): () => void {
    return this.#ports.drafts.onChange(() => {
      this.reloadDrafts();
    });
  }

  /**
   * Reads the server limit and the saved drafts: imports SDK pending uploads
   * as unassigned drafts, drops orphans, adopts accepted sends from the
   * journal, removes published ones and resets interrupted uploads.
   */
  load(): Promise<void> {
    return this.#session(() => this.#load());
  }

  async #load(): Promise<void> {
    try {
      const { gateway, drafts } = this.#ports;
      if (!this.#ports.admit() || !this.#ports.acceptsSession()) return;
      const limit = gateway.capability();
      const pending = await gateway.pending();
      if (!this.#ports.admit() || !this.#ports.acceptsSession()) return;
      drafts.cleanOrphans();
      const saved = drafts.all();
      const discarded = [...this.#discardedFiles.values()];
      for (const file of pending) {
        if (
          discarded.some((entry) => sameFile(entry, file)) ||
          saved.some((draft) => sameFile(draft.file, file))
        ) {
          continue;
        }
        const draft: FileDraft = {
          id: this.#ports.newId(),
          file,
          phase: "waiting",
          retryable: true,
        };
        drafts.save(draft);
        saved.push(draft);
      }
      await this.#reconcileAccepted(saved);
      if (!this.#ports.acceptsSession()) return;
      const current = drafts.all().map((draft) => {
        // An upload a store of this session still runs was not interrupted.
        if (
          draft.phase !== "uploading" ||
          this.#ports.activeSends.has(draft.id)
        ) {
          return draft;
        }
        const waiting: FileDraft = { ...draft, phase: "waiting" };
        drafts.save(waiting);
        return waiting;
      });
      this.#apply((state) => {
        const { limit: _old, ...rest } = state;
        return {
          ...rest,
          ...(limit === undefined ? {} : { limit }),
          loaded: true,
          drafts: current,
        };
      });
    } catch (error) {
      this.#handleFailure(error, { kind: "other" });
    }
  }

  /**
   * Hands a staged copy to the SDK and saves it as a draft of this chat. The
   * MIME type comes from the name, else the picker's type (`pickerType`).
   */
  stage(path: string, filename: string, pickerType?: string): Promise<void> {
    return this.#session(() => this.#stage(path, filename, pickerType));
  }

  async #stage(
    path: string,
    filename: string,
    pickerType: string | undefined,
  ): Promise<void> {
    try {
      try {
        if (
          !this.#ports.allowed() ||
          this.getState().limit === undefined ||
          !this.#ports.admit()
        ) {
          throw new SendError("stale");
        }
        const file = await this.#ports.gateway.create(
          path,
          filename,
          mimeTypeFor(filename, pickerType),
        );
        if (!this.#ports.admit()) return;
        const draft: FileDraft = {
          id: this.#ports.newId(),
          ...(this.#ports.conversationId === undefined
            ? {}
            : { conversationId: this.#ports.conversationId }),
          file,
          phase: "waiting",
          retryable: true,
        };
        this.#ports.drafts.save(draft);
        this.#reloadDrafts();
      } finally {
        this.#ports.removeStaged(path);
      }
    } catch (error) {
      this.#handleFailure(error, { kind: "other" });
    }
  }

  /** Assigns an unassigned draft to a chat (recovery "Assign to …"). */
  assign(id: string, conversationId: string): void {
    try {
      const draft = this.#ports.drafts.all().find((entry) => entry.id === id);
      if (draft === undefined) return;
      this.#ports.drafts.save({ ...draft, conversationId });
      this.#reloadDrafts();
    } catch (error) {
      this.#handleFailure(error, { kind: "other" });
    }
  }

  /** Deletes the SDK's copy and the draft. A queued send cannot start after this. */
  discard(id: string): Promise<void> {
    return this.#session(() => this.#discard(id));
  }

  async #discard(id: string): Promise<void> {
    try {
      if (!this.#ports.acceptsSession()) return;
      const draft = this.#ports.drafts.all().find((entry) => entry.id === id);
      if (draft === undefined) return;
      this.#generations.set(id, (this.#generations.get(id) ?? 0) + 1);
      this.#discarded.add(id);
      this.#discardedFiles.set(id, draft.file);
      try {
        await this.#ports.gateway.discard(draft.file);
        this.#ports.drafts.remove(id);
        this.#apply((state) => ({
          ...state,
          drafts: state.drafts.filter((entry) => entry.id !== id),
        }));
      } catch (error) {
        this.#discarded.delete(id);
        this.#discardedFiles.delete(id);
        throw error;
      }
    } catch (error) {
      this.#handleFailure(error, { kind: "other" });
    }
  }

  /**
   * Sends a draft: an accepted one only publishes again; otherwise it uploads
   * (unless already complete), queues the message, publishes, and removes the
   * draft once the message is published.
   */
  send(id: string): Promise<void> {
    return this.#session(() => this.#send(id));
  }

  async #send(id: string): Promise<void> {
    const { activeSends } = this.#ports;
    if (
      !this.#ports.acceptsSession() ||
      this.#discarded.has(id) ||
      activeSends.has(id)
    ) {
      return;
    }
    activeSends.add(id);
    try {
      const generation = this.#generations.get(id) ?? 0;
      const draft = this.#ports.drafts.all().find((entry) => entry.id === id);
      const conversationId = draft?.conversationId;
      if (
        !this.#ports.allowed() ||
        draft === undefined ||
        conversationId === undefined ||
        draft.phase === "unknown" ||
        (draft.phase === "failed" && !draft.retryable)
      ) {
        return;
      }
      let completed: FileDraft = draft;
      if (draft.acceptedId !== undefined) {
        await this.#ports.coordinator.retry(
          conversationId,
          draft.acceptedId,
          () => this.#canSend(id, generation),
        );
      } else {
        const ready = await this.#uploadIfNeeded(draft, generation);
        if (ready === undefined) return;
        const acceptedId = await this.#queue(ready, conversationId, generation);
        completed = { ...ready, acceptedId };
      }
      await this.#finishSend({ ...completed, phase: "complete" });
    } catch (error) {
      this.#handleFailure(error, { kind: "send", id });
    } finally {
      activeSends.delete(id);
    }
  }

  async #uploadIfNeeded(
    draft: FileDraft,
    generation: number,
  ): Promise<FileDraft | undefined> {
    if (draft.phase === "complete") return draft;
    const uploading: FileDraft = { ...draft, phase: "uploading" };
    this.#ports.drafts.save(uploading);
    this.#reloadDrafts();
    const phase = await this.#ports.gateway.upload(uploading.file);
    if (phase !== "complete") {
      throw new AttachmentError(AttachmentCopy.operationFailed, true);
    }
    // The upload finished, also when the send stops here (consent revoked,
    // the screen closed): save that, so Send queues without uploading again.
    // A draft that was discarded or changed meanwhile stays as it is.
    const complete: FileDraft = { ...uploading, phase: "complete" };
    if (!this.#ports.acceptsSession() || !this.#replace(uploading, complete)) {
      return undefined;
    }
    if (!this.#canSend(draft.id, generation)) {
      this.#reloadDrafts();
      return undefined;
    }
    return complete;
  }

  /** Saves `next` only while the saved draft is still `expected`. */
  #replace(expected: FileDraft, next: FileDraft): boolean {
    const current = this.#ports.drafts
      .all()
      .find((entry) => entry.id === expected.id);
    if (!sameDraft(current, expected)) return false;
    this.#ports.drafts.save(next);
    return true;
  }

  async #queue(
    draft: FileDraft,
    conversationId: string,
    generation: number,
  ): Promise<string> {
    const { drafts, coordinator } = this.#ports;
    try {
      return await coordinator.queue(
        {
          id: draft.id,
          conversationId,
          phase: "queueing",
          createdAt: this.#ports.now(),
        },
        {
          admission: () => this.#canSend(draft.id, generation),
          accept: () => this.#ports.sendFile(conversationId, draft.file),
          beforeQueue: () => {
            drafts.save({ ...draft, phase: "unknown" });
          },
          // A draft discarded while the SDK call ran stays discarded; the
          // journal still records the accepted message.
          acknowledge: (messageId) => {
            this.#replace(
              { ...draft, phase: "unknown" },
              { ...draft, acceptedId: messageId, phase: "complete" },
            );
          },
        },
      );
    } catch (error) {
      if (isKnownRejection(error)) this.#restoreRejected(draft, generation);
      throw error;
    }
  }

  /** A definite rejection: the uploaded draft can be sent again. */
  #restoreRejected(draft: FileDraft, generation: number): void {
    if (
      !this.#ports.acceptsSession() ||
      this.#discarded.has(draft.id) ||
      (this.#generations.get(draft.id) ?? 0) !== generation
    ) {
      return;
    }
    this.#replace({ ...draft, phase: "unknown" }, draft);
  }

  async #finishSend(draft: FileDraft): Promise<void> {
    if (!this.#ports.acceptsSession()) return;
    const current = this.#ports.drafts
      .all()
      .find((entry) => entry.id === draft.id);
    if (
      current === undefined ||
      !sameDraft(current, draft) ||
      current.acceptedId === undefined
    ) {
      this.#reloadDrafts();
      return;
    }
    if (!(await this.#removePublished(current))) {
      if (this.#ports.acceptsSession()) this.#reloadDrafts();
      return;
    }
    this.#apply((state) => ({
      ...state,
      drafts: state.drafts.filter((entry) => entry.id !== draft.id),
    }));
    if (this.#ports.admit()) await this.#ports.changed?.();
  }

  async #reconcileAccepted(saved: FileDraft[]): Promise<void> {
    const { coordinator, drafts } = this.#ports;
    for (let index = saved.length - 1; index >= 0; index -= 1) {
      if (!this.#ports.acceptsSession()) return;
      let draft = saved.at(index);
      if (draft === undefined) continue;
      if (
        draft.acceptedId === undefined &&
        draft.conversationId !== undefined
      ) {
        const messageId = coordinator.acceptedMessageId(
          draft.id,
          draft.conversationId,
        );
        if (messageId !== undefined) {
          draft = { ...draft, acceptedId: messageId, phase: "complete" };
          drafts.save(draft);
          saved[index] = draft;
        }
      }
      if (await this.#removePublished(draft)) saved.splice(index, 1);
    }
  }

  async #removePublished(draft: FileDraft): Promise<boolean> {
    const conversationId = draft.conversationId;
    const messageId = draft.acceptedId;
    if (
      !this.#ports.acceptsSession() ||
      conversationId === undefined ||
      messageId === undefined
    ) {
      return false;
    }
    const published = await this.#ports.published(conversationId, messageId);
    if (!published || !this.#ports.acceptsSession()) return false;
    const current = this.#ports.drafts
      .all()
      .find((entry) => entry.id === draft.id);
    if (!sameDraft(current, draft)) return false;
    this.#ports.drafts.remove(draft.id);
    return true;
  }

  /**
   * Downloads and verifies a message's file. One download per message runs at
   * a time; consent is re-read first; an invalidation drops the result.
   */
  download(messageId: string, file: RemoteAttachment): Promise<void> {
    return this.#session(() => this.#downloadOnce(messageId, file));
  }

  async #downloadOnce(
    messageId: string,
    file: RemoteAttachment,
  ): Promise<void> {
    if (this.#admissions.has(messageId)) return;
    const token = {};
    this.#admissions.set(messageId, token);
    const revision = this.#revision;
    try {
      await this.#download(messageId, file, revision);
    } catch (error) {
      if (revision === this.#revision) {
        this.#handleFailure(error, { kind: "download", messageId, file });
      }
    } finally {
      if (this.#admissions.get(messageId) === token) {
        this.#admissions.delete(messageId);
      }
    }
  }

  #downloadable(messageId: string, revision: number): boolean {
    return (
      revision === this.#revision &&
      this.#ports.allowed() &&
      this.#ports.admit() &&
      this.getState().cards[messageId]?.phase !== "downloading"
    );
  }

  async #download(
    messageId: string,
    file: RemoteAttachment,
    revision: number,
  ): Promise<void> {
    if (!this.#downloadable(messageId, revision)) return;
    const authorize = this.#ports.authorizeDownload;
    if (authorize !== undefined && !(await authorize())) return;
    if (!this.#downloadable(messageId, revision)) return;
    this.#setCard({
      ...defaultCard(messageId, file),
      phase: "downloading",
    });
    const verified = await this.#ports.gateway.download(file);
    if (
      revision !== this.#revision ||
      !this.#ports.admit() ||
      !this.#ports.allowed()
    ) {
      return;
    }
    const evicted = this.#verified.put(messageId, verified);
    this.#apply((state) => {
      const cards: Record<string, FileCard> = Object.fromEntries(
        Object.entries(state.cards).map(([id, card]) => [
          id,
          evicted.includes(id) ? { ...card, phase: "available" } : card,
        ]),
      );
      const base = defaultCard(messageId, file);
      cards[messageId] = {
        ...base,
        name: verified.filename,
        phase: "verified",
      };
      return { ...state, cards };
    });
  }

  #setCard(card: FileCard): void {
    this.#apply((state) => ({
      ...state,
      cards: { ...state.cards, [card.id]: card },
    }));
  }

  /**
   * Consent changed: drops running downloads and verified files, and resets
   * every card to "Download to verify".
   */
  invalidateDownloads(): void {
    this.#revision += 1;
    this.#admissions.clear();
    this.#verified.clear();
    const cards: Record<string, FileCard> = {};
    for (const [id, card] of Object.entries(this.getState().cards)) {
      const { error: _error, ...rest } = card;
      cards[id] = { ...rest, phase: "available", retryable: false };
    }
    this.setState((state) => ({ ...state, cards }));
  }

  /** Shows a screen-local failure on the error line (staging, picking). */
  showError(message: string | undefined): void {
    this.#apply((state) => {
      const { error: _old, ...rest } = state;
      return message === undefined ? rest : { ...rest, error: message };
    });
  }

  #handleFailure(error: unknown, failed: Failed): void {
    if (!this.#ports.acceptsSession()) return;
    const failure = error instanceof AttachmentError ? error : undefined;
    if (failed.kind === "send") this.#recordSendFailure(failed.id, failure);
    const message = failure?.message ?? AttachmentCopy.operationFailed;
    this.showError(message);
    if (failed.kind === "download") {
      this.#setCard({
        ...defaultCard(failed.messageId, failed.file),
        phase: "failed",
        error: message,
        retryable: failure?.retryable ?? true,
      });
    }
  }

  /** An interrupted upload becomes "failed" with the failure's retry rule. */
  #recordSendFailure(id: string, failure: AttachmentError | undefined): void {
    try {
      const draft = this.#ports.drafts.all().find((entry) => entry.id === id);
      if (draft?.phase === "uploading") {
        this.#ports.drafts.save({
          ...draft,
          phase: "failed",
          retryable: failure?.retryable ?? true,
        });
      }
      this.#reloadDrafts();
    } catch {
      // The next load shows the saved state.
    }
  }
}
