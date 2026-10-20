// The session state machine, with storage, secrets and the SDK behind small
// ports so it runs under `node --test`. `src/session` supplies the real ports.
//
// The profile index has one writer at a time. Every index write and every
// fence reservation runs in an index turn (`#indexTurn`): turns run one at a
// time, in call order, and a turn that writes reads the index first. So a
// write never drops another, and a fence check inside a turn holds until the
// turn ends. An explicit action reserves in the same turn as its first index
// write, and an open records its sign-in, or clears it after a cancel, only
// in a turn that its key owns.

import { normalizeBackendUrl } from "./backendUrl.ts";
import { bytesToHex, hexToBytes } from "./hex.ts";
import type { ProfilePaths } from "./paths.ts";
import { continueResetJournal } from "./resetJournal.ts";
import { CancelledError, SessionCopy, SessionError } from "./sessionError.ts";
import { type SessionKey, SessionFence } from "./sessionFence.ts";
import {
  type Appearance,
  type BackendProfile,
  type SessionIndex,
  activeProfile,
  isEstablished,
  profileForUrl,
  withProfile,
} from "./sessionIndex.ts";
import { SessionScope } from "./sessionScope.ts";
import { Store, ValueStore } from "./store.ts";

/** The per-profile secrets. Keys are 32 random bytes stored as 64 hex characters. */
export type SecretRef =
  | "wallet"
  | "database-key"
  | "preferences-key"
  | "credential";

const KEY_REFS = ["wallet", "database-key", "preferences-key"] as const;

/** Device-only secret storage (Keychain or Keystore). Values are text. */
export interface SecretStore {
  read(profileId: string, ref: SecretRef): Promise<string | undefined>;
  write(profileId: string, ref: SecretRef, value: string): Promise<void>;
  remove(profileId: string, ref: SecretRef): Promise<void>;
  removeAll(profileId: string): Promise<void>;
  randomBytes(count: number): Uint8Array;
}

/** The session index and the per-profile directories. */
export interface ProfileFiles {
  /** Creates the storage root. A failure means local storage is unavailable. */
  prepare(): Promise<void>;
  loadIndex(): Promise<SessionIndex>;
  saveIndex(index: SessionIndex): Promise<void>;
  /** The profile's paths, with its directories created and protected. */
  paths(profileId: string): Promise<ProfilePaths>;
  databaseExists(profileId: string): Promise<boolean>;
  removeDatabase(profileId: string): Promise<void>;
  removeExports(profileId: string): Promise<void>;
  removeProfile(profileId: string): Promise<void>;
}

/** The parts of an SDK client that the session machine uses. */
export interface SessionClient {
  readonly inboxId: string;
  /** The signer's Ethereum address. */
  readonly identity: string;
  end(): Promise<void>;
}

/** What the factory needs to open (create or build) a client. */
export interface OpenRequest {
  readonly profile: BackendProfile;
  readonly paths: ProfilePaths;
  /** The signer's private key. Used only when the profile has no inbox yet. */
  readonly wallet: Uint8Array;
  readonly databaseKey: Uint8Array;
  /**
   * Reads the stored access token for each SDK credential request. Undefined
   * when no token is stored: a credential source must not be attached then,
   * because the SDK asks for it even when the server needs no token.
   */
  readonly credential: (() => Promise<string>) | undefined;
  readonly key: SessionKey;
}

export interface ClientFactory<C extends SessionClient> {
  open(request: OpenRequest): Promise<C>;
}

/** A profile in an open session. `name` is the optional name typed at Join. */
export interface ActiveProfile extends BackendProfile {
  readonly inboxId: string;
  readonly identity: string;
  readonly name?: string;
}

/** What `bind` receives when a client opened and the index records it. */
export interface BindRequest<C> {
  readonly client: C;
  readonly key: SessionKey;
  readonly profile: ActiveProfile;
  readonly paths: ProfilePaths;
  /** Session-owned work. The controller cancels and awaits it before `client.end()`. */
  readonly scope: SessionScope;
  readonly isCurrent: () => boolean;
}

export type SessionState<S> =
  | { readonly kind: "starting" }
  | { readonly kind: "storageFailed" }
  | { readonly kind: "signedOut" }
  | { readonly kind: "opening" }
  | { readonly kind: "active"; readonly session: S }
  | { readonly kind: "restoreFailed" }
  | { readonly kind: "resetIncomplete" };

export interface ConnectOptions {
  readonly url: string;
  /** The Access token field. Empty removes a stored token. */
  readonly token: string;
  /** The optional server name, saved as the server label. */
  readonly name?: string;
  /** Aborting it ends a client that opens too late (the user changed the address). */
  readonly signal?: AbortSignal;
  /** Called with the reserved generation before any slow work. */
  readonly onReserved?: (generation: number) => void;
}

export interface SessionControllerOptions<C extends SessionClient, S> {
  readonly secrets: SecretStore;
  readonly files: ProfileFiles;
  readonly factory: ClientFactory<C>;
  /** Builds the active session (stores, streams). Runs once per opened client. */
  readonly bind: (request: BindRequest<C>) => S;
  /** A new profile ID. Defaults to a version 4 UUID from `secrets.randomBytes`. */
  readonly newProfileId?: () => string;
}

const SECRET_HEX = /^[0-9a-f]{64}$/u;

/** `error` as an `Error` (only errors are thrown). */
function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

/** Reads the signal each time (TypeScript narrows `signal.aborted` across awaits). */
function isAborted(signal: AbortSignal | undefined): boolean {
  return signal?.aborted === true;
}

async function endQuietly(client: SessionClient): Promise<void> {
  try {
    await client.end();
  } catch {
    // Ending is best effort; local cleanup must continue.
  }
}

export class SessionController<C extends SessionClient, S> extends Store<
  SessionState<S>
> {
  readonly fence = new SessionFence();
  /** The global appearance setting (survives sign out and deletion). */
  readonly appearance = new ValueStore<Appearance>("system");
  readonly #secrets: SecretStore;
  readonly #files: ProfileFiles;
  readonly #factory: ClientFactory<C>;
  readonly #bind: (request: BindRequest<C>) => S;
  readonly #newProfileId: () => string;
  #client: C | undefined;
  #scope: SessionScope | undefined;
  #closing: Promise<void> | undefined;
  #starting: Promise<void> | undefined;
  #restoring = false;
  #recoveryProfile: BackendProfile | undefined;
  #indexTurns: Promise<unknown> = Promise.resolve();

  constructor(options: SessionControllerOptions<C, S>) {
    super({ kind: "starting" });
    this.#secrets = options.secrets;
    this.#files = options.files;
    this.#factory = options.factory;
    this.#bind = options.bind;
    this.#newProfileId =
      options.newProfileId ?? (() => randomUuid(options.secrets));
  }

  /** The open client, if any (for tests and teardown checks). */
  get client(): C | undefined {
    return this.#client;
  }

  /**
   * Launch: prepares storage, reads the appearance, then restores. Calling it
   * again while it runs returns the same promise; after a storage failure it
   * tries again (the startup Retry).
   */
  start(): Promise<void> {
    const kind = this.getState().kind;
    if (kind !== "starting" && kind !== "storageFailed") {
      return Promise.resolve();
    }
    this.#starting ??= this.#start().finally(() => {
      this.#starting = undefined;
    });
    return this.#starting;
  }

  async #start(): Promise<void> {
    this.setState({ kind: "starting" });
    try {
      await this.#files.prepare();
    } catch {
      this.setState({ kind: "storageFailed" });
      return;
    }
    await this.#refreshAppearance();
    await this.restore();
    if (this.getState().kind === "starting")
      this.setState({ kind: "signedOut" });
  }

  /**
   * Automatic restore at launch and when the app returns to the foreground.
   * It never overrides an explicit user action.
   */
  async restore(): Promise<void> {
    if (this.getState().kind === "active" || this.#restoring) return;
    this.#restoring = true;
    let expected = this.fence.currentGeneration();
    try {
      const index = await this.#files.loadIndex();
      if (index.reset !== undefined) {
        this.setState({ kind: "resetIncomplete" });
        return;
      }
      if (!index.signedIn) return;
      const profile = activeProfile(index);
      if (profile === undefined) {
        this.setState({ kind: "restoreFailed" });
        return;
      }
      this.#recoveryProfile = profile;
      if (!(await this.#files.databaseExists(profile.id))) {
        this.setState({ kind: "restoreFailed" });
        return;
      }
      const generation = await this.#indexTurn(() =>
        Promise.resolve(this.fence.reserveRestore()),
      );
      if (generation === undefined) return;
      expected = generation;
      await this.#open(profile, generation, undefined, false);
    } catch {
      this.#apply(expected, { kind: "restoreFailed" });
    } finally {
      this.#restoring = false;
    }
  }

  /** "Retry" on a failed restore: an explicit action, so it wins over automatic restore. */
  async retryRestore(): Promise<void> {
    if (this.#restoring) return;
    this.#restoring = true;
    const generation = await this.#indexTurn(() =>
      Promise.resolve(this.fence.reserve()),
    );
    try {
      const index = await this.#files.loadIndex();
      if (index.reset !== undefined) {
        this.#apply(generation, { kind: "resetIncomplete" });
        return;
      }
      if (!index.signedIn && this.#recoveryProfile === undefined) {
        this.#apply(generation, { kind: "signedOut" });
        return;
      }
      const profile = this.#recoveryProfile ?? activeProfile(index);
      if (
        profile?.inboxId === undefined ||
        profile.identity === undefined ||
        !(await this.#files.databaseExists(profile.id))
      ) {
        this.#apply(generation, { kind: "restoreFailed" });
        return;
      }
      this.setState({ kind: "opening" });
      await this.#close();
      await this.#open(profile, generation, undefined, false);
    } catch {
      this.#apply(generation, { kind: "restoreFailed" });
    } finally {
      this.#restoring = false;
    }
  }

  /**
   * Join a server. One profile per normalized URL: a known server reopens
   * its identity; a new one creates three secrets and a new inbox.
   */
  async connect(options: ConnectOptions): Promise<void> {
    const backend = normalizeBackendUrl(options.url);
    const { profile, generation } = await this.#indexTurn(async () => {
      const index = await this.#files.loadIndex();
      if (index.reset !== undefined) {
        throw SessionError.message(SessionCopy.finishReset);
      }
      const existing = profileForUrl(index, backend.value);
      const profile: BackendProfile = existing ?? {
        id: this.#newProfileId(),
        url: backend.value,
      };
      const generation = this.fence.reserve();
      options.onReserved?.(generation);
      this.setState({ kind: "opening" });
      try {
        await this.#files.saveIndex({
          ...index,
          signedIn: false,
          activeId: profile.id,
          ...(existing === undefined
            ? { profiles: [...index.profiles, profile] }
            : {}),
        });
      } catch (error) {
        this.#apply(generation, { kind: "signedOut" });
        throw error;
      }
      return { profile, generation };
    });
    let missingDatabase = false;
    try {
      await this.#close();
      const established = isEstablished(profile);
      const hasDatabase = await this.#files.databaseExists(profile.id);
      if (established && !hasDatabase) {
        missingDatabase = true;
        throw SessionError.message(SessionCopy.missingSecret);
      }
      await this.#open(
        profile,
        generation,
        options.token,
        !established && !hasDatabase,
        options.name,
        options.signal,
      );
    } catch (error) {
      const recoverable =
        missingDatabase ||
        (error instanceof SessionError && error.kind === "missingSecret");
      if (this.fence.currentGeneration() === generation && recoverable) {
        this.#recoveryProfile = profile;
      }
      this.#apply(generation, {
        kind: recoverable ? "restoreFailed" : "signedOut",
      });
      throw error;
    }
  }

  /**
   * Sign out: ends the session and removes the access token and exported
   * files. Keys, database and preferences stay, so a rejoin reopens the same
   * inbox. A cleanup failure still signs out, then rethrows.
   */
  async signOut(): Promise<void> {
    // Saved before the reservation: a failed save leaves the session open.
    const { id, generation } = await this.#indexTurn(async () => {
      const index = await this.#files.loadIndex();
      await this.#files.saveIndex({ ...index, signedIn: false });
      return { id: index.activeId, generation: this.fence.reserve() };
    });
    await this.#close();
    if (this.fence.currentGeneration() !== generation) return;
    try {
      if (id === undefined) return;
      let failure: unknown;
      try {
        await this.#secrets.remove(id, "credential");
      } catch (error) {
        failure = error;
      }
      try {
        await this.#files.removeExports(id);
      } catch (error) {
        failure ??= error;
      }
      if (failure !== undefined) throw toError(failure);
    } finally {
      this.#apply(generation, { kind: "signedOut" });
      await this.#refreshAppearance();
    }
  }

  /** "Delete local data": journals the reset, then runs it. */
  async deleteLocalData(): Promise<void> {
    const journaled = await this.#indexTurn(async () => {
      const index = await this.#files.loadIndex();
      const id = index.activeId;
      if (id === undefined) return false;
      await this.#files.saveIndex({
        ...index,
        signedIn: false,
        reset: { profileId: id, phase: "STOPPING" },
      });
      // Like sign out: an open in flight can no longer record a sign-in.
      this.fence.reserve();
      return true;
    });
    if (journaled) await this.continueReset();
  }

  /**
   * Resumes a journaled reset: ends the session, then removes the database,
   * the profile files, the secrets and the profile entry, saving each phase.
   */
  async continueReset(): Promise<void> {
    const reset = await this.#indexTurn(async () => {
      const journal = (await this.#files.loadIndex()).reset;
      return journal === undefined
        ? undefined
        : { journal, generation: this.fence.reserve() };
    });
    if (reset === undefined) return;
    const { journal, generation } = reset;
    try {
      await this.#close();
      if (this.fence.currentGeneration() !== generation) {
        throw SessionError.stale();
      }
      await continueResetJournal(journal, {
        removeDatabase: (id) => this.#files.removeDatabase(id),
        removeProfileFiles: (id) => this.#files.removeProfile(id),
        removeSecrets: (id) => this.#secrets.removeAll(id),
        saveJournal: (next) =>
          this.#updateIndex(generation, (index) => ({ ...index, reset: next })),
        finish: (id) =>
          this.#updateIndex(generation, (index) => {
            const { reset: _reset, activeId, ...rest } = index;
            return {
              ...rest,
              profiles: index.profiles.filter((profile) => profile.id !== id),
              ...(activeId === undefined || activeId === id
                ? {}
                : { activeId }),
            };
          }),
      });
      this.#apply(generation, { kind: "signedOut" });
      await this.#refreshAppearance();
    } catch (error) {
      this.#apply(generation, { kind: "resetIncomplete" });
      throw error;
    }
  }

  /** Saves the appearance in the global index. */
  async setAppearance(value: Appearance): Promise<void> {
    try {
      await this.#indexTurn(async () => {
        const index = await this.#files.loadIndex();
        await this.#files.saveIndex({ ...index, appearance: value });
      });
    } catch {
      throw SessionError.message(SessionCopy.appearanceFailed);
    }
    this.appearance.set(value);
  }

  async #refreshAppearance(): Promise<void> {
    try {
      this.appearance.set((await this.#files.loadIndex()).appearance);
    } catch {
      // Restore reports an unreadable index; this only styles the app.
    }
  }

  #apply(generation: number, state: SessionState<S>): void {
    if (this.fence.currentGeneration() === generation) this.setState(state);
  }

  /**
   * Runs `work` when the index turns before it have settled. No other index
   * write or fence reservation runs until `work` settles, so `work` must not
   * wait for another turn.
   */
  #indexTurn<T>(work: () => Promise<T>): Promise<T> {
    const turn = this.#indexTurns.then(work);
    this.#indexTurns = turn.catch(() => undefined);
    return turn;
  }

  /** Saves `change` of the current index while `generation` is current. */
  #updateIndex(
    generation: number,
    change: (index: SessionIndex) => SessionIndex,
  ): Promise<void> {
    return this.#indexTurn(async () => {
      if (this.fence.currentGeneration() !== generation) {
        throw SessionError.stale();
      }
      await this.#files.saveIndex(change(await this.#files.loadIndex()));
    });
  }

  /** Ends the open session: cancels and awaits its work, then ends the client. */
  async #close(): Promise<void> {
    if (this.#closing !== undefined) {
      await this.#closing;
      return;
    }
    const client = this.#client;
    const scope = this.#scope;
    this.#client = undefined;
    this.#scope = undefined;
    if (client === undefined && scope === undefined) return;
    this.#closing = (async () => {
      await scope?.cancelAndWait();
      if (client !== undefined) await endQuietly(client);
    })().finally(() => {
      this.#closing = undefined;
    });
    await this.#closing;
  }

  async #secret(
    profileId: string,
    ref: (typeof KEY_REFS)[number],
    create: boolean,
  ): Promise<Uint8Array> {
    const saved = await this.#secrets.read(profileId, ref);
    if (saved !== undefined) {
      const bytes = SECRET_HEX.test(saved) ? hexToBytes(saved) : undefined;
      if (bytes === undefined) throw SessionError.missingSecret();
      return bytes;
    }
    if (!create) throw SessionError.missingSecret();
    const bytes = this.#secrets.randomBytes(32);
    await this.#secrets.write(profileId, ref, bytesToHex(bytes));
    return bytes;
  }

  async #open(
    profile: BackendProfile,
    generation: number,
    token: string | undefined,
    createSecrets: boolean,
    name?: string,
    signal?: AbortSignal,
  ): Promise<void> {
    if (isAborted(signal)) throw new CancelledError();
    const key = this.fence.bind(profile.id, generation);
    if (key === undefined) throw SessionError.stale();
    this.setState({ kind: "opening" });
    if (token !== undefined) {
      if (token === "") await this.#secrets.remove(profile.id, "credential");
      else await this.#secrets.write(profile.id, "credential", token);
    }
    // Secrets are written before the SDK call, so a failed create retries
    // with the same wallet.
    const wallet = await this.#secret(profile.id, "wallet", createSecrets);
    const databaseKey = await this.#secret(
      profile.id,
      "database-key",
      createSecrets,
    );
    await this.#secret(profile.id, "preferences-key", createSecrets);
    const hasCredential =
      (await this.#secrets.read(profile.id, "credential")) !== undefined;
    const paths = await this.#files.paths(profile.id);
    const opened = await this.#factory.open({
      profile,
      paths,
      wallet,
      databaseKey,
      credential: hasCredential ? () => this.#credential(key) : undefined,
      key,
    });
    if (!this.fence.accepts(key)) {
      await endQuietly(opened);
      throw SessionError.stale();
    }
    if (isAborted(signal)) {
      await endQuietly(opened);
      throw new CancelledError();
    }
    const saved: ActiveProfile = {
      ...profile,
      inboxId: opened.inboxId,
      identity: opened.identity,
      ...(name === undefined || name === "" ? {} : { name }),
    };
    let committed: "signedIn" | "cancelled";
    try {
      committed = await this.#indexTurn(async () => {
        // Turns before this one may have replaced or cancelled the attempt.
        if (!this.fence.accepts(key)) throw SessionError.stale();
        if (isAborted(signal)) throw new CancelledError();
        const { name: _name, ...stored } = saved;
        const signedIn: SessionIndex = {
          ...withProfile(await this.#files.loadIndex(), stored),
          activeId: saved.id,
          signedIn: true,
        };
        await this.#files.saveIndex(signedIn);
        if (!isAborted(signal)) return "signedIn";
        // The caller cancelled during the write. This turn still owns the
        // index, so it clears the sign-in it saved. If that write fails, the
        // index keeps the sign-in and the session opens: the state then
        // matches the index, which the next launch restores, and Sign out
        // stays available.
        try {
          await this.#files.saveIndex({ ...signedIn, signedIn: false });
          return "cancelled";
        } catch {
          return "signedIn";
        }
      });
    } catch (error) {
      await endQuietly(opened);
      throw error;
    }
    if (committed === "cancelled") {
      await endQuietly(opened);
      throw new CancelledError();
    }
    // An explicit action may have reserved since the turn ended. It now owns
    // the index, which records this sign-in.
    if (!this.fence.accepts(key)) {
      await endQuietly(opened);
      throw SessionError.stale();
    }
    const scope = new SessionScope();
    let session: S;
    try {
      session = this.#bind({
        client: opened,
        key,
        profile: saved,
        paths,
        scope,
        isCurrent: () => this.fence.accepts(key),
      });
    } catch (error) {
      await scope.cancelAndWait();
      await endQuietly(opened);
      throw error;
    }
    this.#client = opened;
    this.#scope = scope;
    this.#recoveryProfile = undefined;
    this.#apply(generation, { kind: "active", session });
  }

  async #credential(key: SessionKey): Promise<string> {
    if (!this.fence.accepts(key)) throw SessionError.stale();
    const value = await this.#secrets.read(key.profileId, "credential");
    if (value === undefined || value === "" || !this.fence.accepts(key)) {
      throw SessionError.stale();
    }
    return value;
  }
}

/** A random version 4 UUID from the secret store's random source. */
function randomUuid(secrets: SecretStore): string {
  const bytes = secrets.randomBytes(16);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytesToHex(bytes);
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20),
  ]
    .join("-")
    .toUpperCase();
}
