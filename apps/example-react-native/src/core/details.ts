// Conversation details, member mutations, permission presets and pending
// membership changes (ExampleCore/Details/DetailsStore.swift and
// DetailsModels.swift). The SDK side is a `GroupQueries` port.

import {
  type Authority,
  canChangePermissionPreset,
  type GroupRole,
  type PermissionWrite,
  permissionPresetWrites,
} from "./permissions.ts";
import { RecipientError } from "./recipients.ts";
import { Store } from "./store.ts";
import type { TitleSource } from "./titles.ts";

export const DetailsCopy = {
  updateFailed: "Cannot update this conversation. Try again.",
  recipientNotFound: "No XMTP account matches this recipient.",
} as const;

/** A group member (`GroupMember`). */
export interface DetailsMember {
  readonly id: string;
  readonly handle?: string;
  readonly otherHandles: readonly string[];
  readonly role: GroupRole;
}

/** A loaded conversation (`ConversationDetails`). */
export interface ConversationDetails extends Authority {
  readonly record: TitleSource;
  readonly description: string;
  readonly members: readonly DetailsMember[];
  readonly policies: readonly PermissionWrite[];
  /** "All members", "Admins only" or "Custom". */
  readonly preset: string;
  readonly retentionNs: bigint;
  readonly pendingLeave: boolean;
}

export type DetailsMutation =
  | { readonly kind: "add"; readonly input: string }
  | { readonly kind: "remove"; readonly id: string }
  | { readonly kind: "admin"; readonly id: string; readonly enabled: boolean }
  | {
      readonly kind: "edit";
      readonly name: string;
      readonly description: string;
    }
  | { readonly kind: "leave" }
  | { readonly kind: "block" }
  | { readonly kind: "disappearing"; readonly retentionNs?: bigint };

export type DetailsAction =
  | { readonly kind: "refresh" }
  | { readonly kind: "mutate"; readonly mutation: DetailsMutation }
  | { readonly kind: "preset"; readonly adminOnly: boolean };

/** The SDK side of the store (`GroupQueries`). */
export interface GroupQueries {
  load(): Promise<ConversationDetails>;
  /** Resolves a recipient; throws `RecipientError` with the iOS copy. */
  resolve(input: string): Promise<DetailsMember>;
  /** Writes one mutation. `add` receives a resolved inbox ID as `input`. */
  mutate(mutation: DetailsMutation): Promise<void>;
  permission(write: PermissionWrite): Promise<void>;
}

export interface DetailsState {
  readonly details?: ConversationDetails;
  /** The Add member preview. */
  readonly recipient?: DetailsMember;
  readonly busy: boolean;
  readonly error?: string;
  readonly pendingMembers: ReadonlyMap<string, DetailsMember>;
  readonly pendingRoles: ReadonlyMap<string, GroupRole>;
  readonly pendingRemoval: ReadonlySet<string>;
  readonly pendingLeave: boolean;
  readonly blocked: boolean;
}

const initialState: DetailsState = {
  busy: false,
  pendingMembers: new Map(),
  pendingRoles: new Map(),
  pendingRemoval: new Set(),
  pendingLeave: false,
  blocked: false,
};

/** The error of a load that never produced details (Retry is offered). */
export function initialLoadError(state: DetailsState): string | undefined {
  return state.details === undefined && !state.busy ? state.error : undefined;
}

/** True while a membership change waits for the group to confirm it. */
export function hasPending(state: DetailsState): boolean {
  return (
    state.pendingMembers.size > 0 ||
    state.pendingRoles.size > 0 ||
    state.pendingRemoval.size > 0 ||
    state.pendingLeave
  );
}

/** A member's role change or removal is pending. */
export function memberPending(state: DetailsState, id: string): boolean {
  return state.pendingRoles.has(id) || state.pendingRemoval.has(id);
}

/** Thrown inside the store when its result must be dropped. */
class Dropped extends Error {
  constructor() {
    super("dropped");
    this.name = "Dropped";
  }
}

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

export interface DetailsStoreOptions {
  /** False once the session or screen that owns the store has ended. */
  readonly admit?: () => boolean;
  /** Runs session-owned work (the session awaits it before it ends). */
  readonly run?: (job: () => Promise<void>) => void;
  /** Waits between pending-change reloads (1 s). */
  readonly pause?: () => Promise<void>;
  /** Waits before a recipient lookup (300 ms). */
  readonly debounce?: () => Promise<void>;
}

/**
 * One screen's view of a conversation. One action runs at a time; every
 * action reloads after it (also after a failure, so a partial preset shows
 * the committed policies). While a membership change is pending it reloads
 * every second until the group confirms it.
 */
export class DetailsStore extends Store<DetailsState> {
  readonly #queries: GroupQueries;
  readonly #own: string;
  readonly #admit: () => boolean;
  readonly #run: (job: () => Promise<void>) => void;
  readonly #pause: () => Promise<void>;
  readonly #debounce: () => Promise<void>;
  #disposed = false;
  #lookup = 0;
  #polling = false;

  constructor(
    queries: GroupQueries,
    ownInboxId: string,
    options: DetailsStoreOptions = {},
  ) {
    super(initialState);
    this.#queries = queries;
    this.#own = ownInboxId;
    this.#admit = options.admit ?? (() => true);
    this.#run =
      options.run ??
      ((job) => {
        void job();
      });
    this.#pause = options.pause ?? (() => sleep(1000));
    this.#debounce = options.debounce ?? (() => sleep(300));
  }

  get ownInboxId(): string {
    return this.#own;
  }

  /** Only a super admin changes the preset. */
  get canChangePermissionPreset(): boolean {
    const details = this.getState().details;
    return (
      details !== undefined && canChangePermissionPreset(details, this.#own)
    );
  }

  /** True after `dispose`. */
  get disposed(): boolean {
    return this.#disposed;
  }

  /** Stops the store: no result applies after this. */
  dispose(): void {
    this.#disposed = true;
  }

  #admits(): boolean {
    return !this.#disposed && this.#admit();
  }

  #apply(change: (state: DetailsState) => DetailsState): void {
    if (this.#admits()) this.setState(change);
  }

  /**
   * Looks up the Add member input after 300 ms. A newer input or an empty
   * one drops the attempt.
   */
  resolvePreview(input: string): void {
    this.#lookup += 1;
    const attempt = this.#lookup;
    this.#apply((state) => {
      const { recipient: _recipient, error: _error, ...rest } = state;
      return rest;
    });
    if (input.trim() === "") return;
    this.#run(async () => {
      await this.#debounce();
      if (attempt !== this.#lookup || !this.#admits()) return;
      try {
        const recipient = await this.#queries.resolve(input);
        if (attempt !== this.#lookup) return;
        this.#apply((state) => ({ ...state, recipient }));
      } catch (error) {
        if (attempt !== this.#lookup) return;
        this.#apply((state) => ({
          ...state,
          error:
            error instanceof RecipientError
              ? error.message
              : DetailsCopy.recipientNotFound,
        }));
      }
    });
  }

  /** Starts an action unless one is running. Presets need a super admin. */
  send(action: DetailsAction): void {
    if (this.getState().busy || !this.#admits()) return;
    if (action.kind === "preset" && !this.canChangePermissionPreset) return;
    this.setState((state) => {
      const { error: _error, ...rest } = state;
      return { ...rest, busy: true };
    });
    this.#run(() => this.#perform(action));
  }

  async #reload(): Promise<void> {
    if (!this.#admits()) throw new Dropped();
    const details = await this.#queries.load();
    this.#apply((state) => {
      const ids = new Set(details.members.map((member) => member.id));
      const roles = new Map(
        details.members.map((member) => [member.id, member.role]),
      );
      return {
        ...state,
        details,
        pendingMembers: new Map(
          [...state.pendingMembers].filter(([id]) => !ids.has(id)),
        ),
        pendingRoles: new Map(
          [...state.pendingRoles].filter(([id, role]) => {
            const current = roles.get(id);
            return current !== undefined && current !== role;
          }),
        ),
        pendingRemoval: new Set(
          [...state.pendingRemoval].filter((id) => ids.has(id)),
        ),
        pendingLeave: details.pendingLeave,
      };
    });
  }

  async #perform(action: DetailsAction): Promise<void> {
    try {
      if (action.kind === "preset") {
        for (const write of permissionPresetWrites(action.adminOnly)) {
          if (!this.#admits() || !this.canChangePermissionPreset) {
            throw new Dropped();
          }
          await this.#queries.permission(write);
        }
      } else if (action.kind === "mutate") {
        await this.#mutate(action.mutation);
      }
      await this.#reload();
    } catch (error) {
      if (!(error instanceof Dropped)) {
        // A preset is several commits: show what the group has now.
        try {
          await this.#reload();
        } catch {
          // The error below is what the screen shows.
        }
        this.#apply((state) => ({
          ...state,
          error:
            error instanceof RecipientError
              ? error.message
              : DetailsCopy.updateFailed,
        }));
      }
    }
    this.#apply((state) => ({ ...state, busy: false }));
    await this.#pollPending();
  }

  async #mutate(mutation: DetailsMutation): Promise<void> {
    let resolved = mutation;
    let added: DetailsMember | undefined;
    if (mutation.kind === "add") {
      added = await this.#queries.resolve(mutation.input);
      resolved = { kind: "add", input: added.id };
    }
    if (!this.#admits()) throw new Dropped();
    await this.#queries.mutate(resolved);
    this.#recordPending(resolved, added);
  }

  #recordPending(
    mutation: DetailsMutation,
    added: DetailsMember | undefined,
  ): void {
    this.#apply((state) => {
      let next = state;
      if (added !== undefined) {
        next = {
          ...next,
          pendingMembers: new Map(next.pendingMembers).set(added.id, added),
        };
      }
      switch (mutation.kind) {
        case "remove":
          return {
            ...next,
            pendingRemoval: new Set(next.pendingRemoval).add(mutation.id),
          };
        case "admin":
          return {
            ...next,
            pendingRoles: new Map(next.pendingRoles).set(
              mutation.id,
              mutation.enabled ? "admin" : "member",
            ),
          };
        case "leave":
          return { ...next, pendingLeave: true };
        case "block":
          return { ...next, blocked: true };
        default:
          return next;
      }
    });
  }

  /** One poller per store: reloads every second while a change is pending. */
  async #pollPending(): Promise<void> {
    if (this.#polling) return;
    this.#polling = true;
    try {
      while (this.#admits() && hasPending(this.getState())) {
        await this.#pause();
        if (!this.#admits()) return;
        try {
          await this.#reload();
        } catch {
          return;
        }
      }
    } finally {
      this.#polling = false;
    }
  }
}
