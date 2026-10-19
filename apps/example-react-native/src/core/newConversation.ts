// New message: recipient resolution and the two-step flow.

import {
  RecipientError,
  parseRecipient,
  recipientEntries,
  recipientNotFound,
} from "./recipients.ts";
import { Store } from "./store.ts";

/** A resolved person: an inbox and its current handle, if any. */
export interface Person {
  readonly inboxId: string;
  readonly handle?: string;
}

/** `@handle`, else the first 12 inbox ID characters. */
export function personLabel(person: Person): string {
  return person.handle === undefined
    ? person.inboxId.slice(0, 12)
    : "@" + person.handle;
}

export const NewConversationCopy = {
  noRecipients: "Add at least one recipient.",
  dmNeedsOne: "A direct message needs one recipient.",
  tooMany: "This group has too many members.",
  failed: "Cannot complete this request. Try again.",
  loadFailed: "Cannot load people. Try again.",
} as const;

/** An error whose message is the copy to show. */
export class NewConversationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NewConversationError";
  }
}

/** The identity reads that resolution needs (`client.inboxIdFor`, `client.inboxStates`). */
export interface IdentityPort {
  /** The inbox of a handle or an Ethereum address, or undefined. */
  inboxIdFor(identity: {
    readonly kind: "handle" | "ethereum";
    readonly identifier: string;
  }): Promise<string | undefined>;
  inboxStates(
    inboxIds: readonly string[],
    refreshFromNetwork: boolean,
  ): Promise<
    readonly {
      readonly inboxId: string;
      readonly identities: readonly {
        readonly kind: string;
        readonly identifier: string;
      }[];
      readonly handle?: string;
    }[]
  >;
  /** True for the SDK's `IdentityNotFound`. */
  isNotFound(error: unknown): boolean;
}

/**
 * The inbox of one recipient entry. An inbox ID is accepted as is (no network
 * check). A handle or an address is looked up; an address must also still be
 * listed on that inbox (`inboxStates([inboxId], true)`). A missing
 * inbox gives the not-found copy.
 */
export async function resolveInboxId(
  input: string,
  identity: IdentityPort,
): Promise<string> {
  const parsed = parseRecipient(input);
  if (parsed.kind === "inbox") return parsed.inboxId;
  const identifier = parsed.kind === "handle" ? parsed.handle : parsed.address;
  let inboxId: string | undefined;
  try {
    inboxId = await identity.inboxIdFor({ kind: parsed.kind, identifier });
    if (inboxId !== undefined && parsed.kind === "ethereum") {
      const states = await identity.inboxStates([inboxId], true);
      const found = inboxId;
      const listed = states.some(
        (state) =>
          state.inboxId === found &&
          state.identities.some(
            (member) =>
              member.kind === "ethereum" &&
              member.identifier.toLowerCase() === identifier.toLowerCase(),
          ),
      );
      if (!listed) inboxId = undefined;
    }
  } catch (error) {
    if (!identity.isNotFound(error)) throw error;
    inboxId = undefined;
  }
  if (inboxId === undefined) throw recipientNotFound(parsed);
  return inboxId;
}

/**
 * One recipient entry as a person: its inbox and its handle
 * (`inboxStates([inboxId], false)`; a failed read gives no handle).
 * Parse and lookup failures throw `NewConversationError` with the
 * `RecipientCopy` message.
 */
export async function resolvePerson(
  input: string,
  identity: IdentityPort,
): Promise<Person> {
  let inboxId: string;
  try {
    inboxId = await resolveInboxId(input, identity);
  } catch (error) {
    if (error instanceof RecipientError) {
      throw new NewConversationError(error.message);
    }
    throw error;
  }
  let handle: string | undefined;
  try {
    handle = (await identity.inboxStates([inboxId], false))[0]?.handle;
  } catch {
    handle = undefined;
  }
  return handle === undefined ? { inboxId } : { inboxId, handle };
}

/** What the store needs from the SDK. */
export interface NewConversationQueries {
  /** Members of up to 20 allowed conversations, without you, at most 100. */
  people(): Promise<readonly Person[]>;
  /** The server's group size limit, including you. */
  maximumMembers(): Promise<number>;
  resolve(input: string): Promise<Person>;
  /** Creates the conversation and returns its ID. */
  create(request: {
    readonly inboxIds: readonly string[];
    readonly group: boolean;
    readonly name: string;
    readonly description: string;
    readonly adminOnly: boolean;
  }): Promise<string>;
}

export interface NewConversationState {
  readonly group: boolean;
  readonly input: string;
  readonly recipients: readonly Person[];
  readonly people: readonly Person[];
  readonly name: string;
  readonly description: string;
  readonly adminOnly: boolean;
  readonly reviewing: boolean;
  readonly busy: boolean;
  /**
   * True while the SDK create call runs. A create already sent cannot be
   * taken back, so the sheet cannot be dismissed until it ends.
   */
  readonly creating: boolean;
  readonly loadingPeople: boolean;
  readonly error?: string;
  readonly bootstrapError?: string;
  readonly maximumMembers: number;
}

export const initialNewConversationState: NewConversationState = {
  group: false,
  input: "",
  recipients: [],
  people: [],
  name: "",
  description: "",
  adminOnly: false,
  reviewing: false,
  busy: false,
  creating: false,
  loadingPeople: false,
  maximumMembers: 0,
};

/**
 * Next and Create are enabled when nothing runs, a group knows its size
 * limit, and there is a recipient or typed input.
 */
export function canContinue(state: NewConversationState): boolean {
  return (
    !state.busy &&
    !state.loadingPeople &&
    (!state.group || state.maximumMembers > 0) &&
    (state.recipients.length > 0 || state.input.trim() !== "")
  );
}

/** True when the Add button is disabled. */
export function addDisabled(state: NewConversationState): boolean {
  return state.input.trim() === "" || state.busy;
}

/** "Members · N": the recipients and you. */
export function memberCount(state: NewConversationState): number {
  return state.recipients.length + 1;
}

/** Thrown inside a run that a newer action replaced. */
class Superseded extends Error {}

export interface NewConversationOptions {
  /** False once the session that opened the sheet ended. */
  readonly admit: () => boolean;
  /** Called with the new conversation's ID: close the sheet and open the chat. */
  readonly finish: (conversationId: string) => void;
}

type Step = "add" | "next" | "create";

/**
 * The New message flow. Every edit cancels resolution in flight (its result
 * is dropped). Add, Next and Create resolve every comma-separated entry in
 * order before any write, skip people already chosen, then check the count;
 * Next then shows the review and Create creates the conversation. Add, Next
 * and Create are ignored while one of them runs. A create already sent
 * cannot be taken back: it stays busy until it ends, also when the review is
 * left or the people change, and then opens the new chat.
 */
export class NewConversationStore extends Store<NewConversationState> {
  readonly #queries: NewConversationQueries;
  readonly #options: NewConversationOptions;
  #closed = false;
  #revision = 0;
  #loadRevision = 0;

  constructor(
    queries: NewConversationQueries,
    options: NewConversationOptions,
  ) {
    super(initialNewConversationState);
    this.#queries = queries;
    this.#options = options;
  }

  /** Stops the store: later actions and results are ignored. */
  dispose(): void {
    this.#closed = true;
    this.#revision += 1;
    this.#loadRevision += 1;
  }

  #open(): boolean {
    return !this.#closed && this.#options.admit();
  }

  #update(change: (state: NewConversationState) => NewConversationState): void {
    this.setState(change);
  }

  /** Loads "People from your chats" and the group size limit. */
  load(): void {
    if (!this.#open()) return;
    this.#loadRevision += 1;
    const expected = this.#loadRevision;
    this.#update((state) => {
      const { bootstrapError: _cleared, ...rest } = state;
      return { ...rest, loadingPeople: true };
    });
    const admits = () => expected === this.#loadRevision && this.#open();
    void (async () => {
      try {
        // Like a SwiftUI task, the work starts after the current action.
        await Promise.resolve();
        if (!admits()) return;
        const people = await this.#queries.people();
        if (!admits()) return;
        const maximumMembers = await this.#queries.maximumMembers();
        if (!admits()) return;
        this.#update((state) => {
          const { bootstrapError: _cleared, ...rest } = state;
          return { ...rest, people, maximumMembers, loadingPeople: false };
        });
      } catch {
        if (!admits()) return;
        this.#update((state) => ({
          ...state,
          loadingPeople: false,
          bootstrapError: NewConversationCopy.loadFailed,
        }));
      }
    })();
  }

  /** Direct message or Group. Leaves the review. */
  setGroup(group: boolean): void {
    if (!this.#open()) return;
    this.#cancel((state) => ({ ...state, group, reviewing: false }));
  }

  setInput(input: string): void {
    if (!this.#open() || this.getState().input === input) return;
    this.#cancel((state) => ({ ...state, input }));
  }

  remove(inboxId: string): void {
    if (!this.#open()) return;
    this.#cancel((state) => ({
      ...state,
      recipients: state.recipients.filter(
        (person) => person.inboxId !== inboxId,
      ),
    }));
  }

  /** Selects or deselects a person from your chats (no checks). */
  toggle(person: Person): void {
    if (!this.#open()) return;
    this.#cancel((state) => ({
      ...state,
      recipients: state.recipients.some(
        (chosen) => chosen.inboxId === person.inboxId,
      )
        ? state.recipients.filter((chosen) => chosen.inboxId !== person.inboxId)
        : [...state.recipients, person],
    }));
  }

  /** Leaves the review (Back, Change people, a swipe back). */
  back(): void {
    if (!this.#open()) return;
    this.#cancel((state) => ({ ...state, reviewing: false }));
  }

  setName(name: string): void {
    this.#setDetails((state) => ({ ...state, name }));
  }

  setDescription(description: string): void {
    this.#setDetails((state) => ({ ...state, description }));
  }

  setAdminOnly(adminOnly: boolean): void {
    this.#setDetails((state) => ({ ...state, adminOnly }));
  }

  /**
   * Changes the group's name, description or permissions. Ignored while busy:
   * Create already sent them, and the sheet closes when it succeeds.
   */
  #setDetails(
    change: (state: NewConversationState) => NewConversationState,
  ): void {
    if (!this.#open() || this.getState().busy) return;
    this.#update(change);
  }

  add(): void {
    this.#run("add");
  }

  next(): void {
    this.#run("next");
  }

  create(): void {
    this.#run("create");
  }

  /**
   * Drops work in flight, clears `busy` and `error`, then applies `change`.
   * A running create call is kept, and stays busy.
   */
  #cancel(change: (state: NewConversationState) => NewConversationState): void {
    if (!this.getState().creating) this.#revision += 1;
    this.#update((state) => {
      const { error: _cleared, ...rest } = state;
      return change({ ...rest, busy: state.creating });
    });
  }

  #run(step: Step): void {
    // A second tap before the disabled button renders must not start a
    // second run: a create already sent cannot be taken back.
    if (!this.#open() || this.getState().busy) return;
    this.#cancel((state) => ({ ...state, busy: true }));
    const revision = this.#revision;
    const { input, recipients } = this.getState();
    const check = (): void => {
      if (revision !== this.#revision || !this.#open()) throw new Superseded();
    };
    void (async () => {
      try {
        await Promise.resolve();
        const resolved = [...recipients];
        for (const entry of recipientEntries(input)) {
          check();
          const person = await this.#queries.resolve(entry);
          if (!resolved.some((chosen) => chosen.inboxId === person.inboxId)) {
            resolved.push(person);
          }
        }
        check();
        this.#validate(resolved);
        this.#update((state) => ({
          ...state,
          recipients: resolved,
          input: "",
          reviewing: step === "next" ? true : state.reviewing,
        }));
        if (step === "create") {
          check();
          const state = this.getState();
          this.#update((current) => ({ ...current, creating: true }));
          let id: string;
          try {
            id = await this.#queries.create({
              inboxIds: resolved.map((person) => person.inboxId),
              group: state.group,
              name: state.name,
              description: state.description,
              adminOnly: state.adminOnly,
            });
          } finally {
            this.#update((current) => ({ ...current, creating: false }));
          }
          check();
          this.#update((current) => ({ ...current, busy: false }));
          this.#options.finish(id);
          return;
        }
        this.#update((state) => ({ ...state, busy: false }));
      } catch (error) {
        if (error instanceof Superseded) return;
        if (revision !== this.#revision || !this.#open()) return;
        this.#update((state) => ({
          ...state,
          busy: false,
          error:
            error instanceof NewConversationError
              ? error.message
              : NewConversationCopy.failed,
        }));
      }
    })();
  }

  #validate(recipients: readonly Person[]): void {
    const { group, maximumMembers } = this.getState();
    if (recipients.length === 0) {
      throw new NewConversationError(NewConversationCopy.noRecipients);
    }
    if (!group && recipients.length !== 1) {
      throw new NewConversationError(NewConversationCopy.dmNeedsOne);
    }
    if (group && recipients.length + 1 > maximumMembers) {
      throw new NewConversationError(NewConversationCopy.tooMany);
    }
  }
}
