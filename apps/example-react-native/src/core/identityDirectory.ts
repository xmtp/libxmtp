// Inbox → handle cache for one session (ExampleCore/Identity/
// IdentityDirectory.swift). Handle-changed events win over reads that started
// before them.

import { Store } from "./store.ts";

export interface InboxIdentity {
  readonly inboxId: string;
  readonly handle?: string;
  readonly otherHandles: readonly string[];
}

/** Reads the handles of inboxes (`client.inboxStates(ids, false)`). */
export type IdentityLookup = (
  inboxIds: readonly string[],
) => Promise<readonly InboxIdentity[]>;

export class IdentityDirectory extends Store<
  ReadonlyMap<string, InboxIdentity>
> {
  readonly #lookup: IdentityLookup;
  readonly #valid: () => boolean;
  readonly #revisions = new Map<string, number>();

  /** `valid` is false once the session ended; nothing merges after that. */
  constructor(lookup: IdentityLookup, valid: () => boolean) {
    super(new Map());
    this.#lookup = lookup;
    this.#valid = valid;
  }

  identity(inboxId: string): InboxIdentity | undefined {
    return this.getState().get(inboxId);
  }

  merge(entries: readonly InboxIdentity[]): void {
    if (!this.#valid() || entries.length === 0) return;
    const next = new Map(this.getState());
    for (const entry of entries) {
      next.set(entry.inboxId, entry);
      this.#revisions.set(
        entry.inboxId,
        (this.#revisions.get(entry.inboxId) ?? 0) + 1,
      );
    }
    this.setState(next);
  }

  /**
   * A handle change from the event stream: the previous handle joins the
   * previous others, deduplicated.
   */
  handleChanged(inboxId: string, handle: string | undefined): void {
    const previous = this.identity(inboxId);
    const candidates = [
      ...(previous?.handle === undefined ? [] : [previous.handle]),
      ...(previous?.otherHandles ?? []),
    ];
    const others = [...new Set(candidates)].filter((entry) => entry !== handle);
    this.merge([
      {
        inboxId,
        ...(handle === undefined ? {} : { handle }),
        otherHandles: others,
      },
    ]);
  }

  /** Reads inboxes; an entry changed meanwhile (by an event) is kept. */
  async load(inboxIds: readonly string[]): Promise<void> {
    const captured = new Map(
      [...new Set(inboxIds)].map((id) => [id, this.#revisions.get(id) ?? 0]),
    );
    const entries = await this.#lookup(inboxIds);
    this.merge(
      entries.filter(
        (entry) =>
          captured.get(entry.inboxId) ===
          (this.#revisions.get(entry.inboxId) ?? 0),
      ),
    );
  }
}
