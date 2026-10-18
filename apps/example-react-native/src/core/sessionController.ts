// The session state machine (ExampleCore/Session/SessionController.swift),
// with storage, secrets and the SDK behind small ports so it runs under
// `node --test`. `src/session` supplies the real ports.

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
  readonly fence?: SessionFence;
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
  readonly fence: SessionFence;
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

  constructor(options: SessionControllerOptions<C, S>) {
    super({ kind: "starting" });
    this.fence = options.fence ?? new SessionFence();
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

  /** True while `key` is the open session. */
  isCurrent(key: SessionKey): boolean {
    return this.fence.accepts(key);
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
      const generation = this.fence.reserveRestore();
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
    const generation = this.fence.reserve();
    this.#restoring = true;
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
    let index = await this.#files.loadIndex();
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
    let missingDatabase = false;
    try {
      index = { ...index, signedIn: false, activeId: profile.id };
      if (existing === undefined) {
        index = { ...index, profiles: [...index.profiles, profile] };
      }
      await this.#files.saveIndex(index);
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
    const index = await this.#files.loadIndex();
    await this.#files.saveIndex({ ...index, signedIn: false });
    const generation = this.fence.reserve();
    await this.#close();
    if (this.fence.currentGeneration() !== generation) return;
    try {
      const id = index.activeId;
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
    const index = await this.#files.loadIndex();
    const id = index.activeId;
    if (id === undefined) return;
    await this.#files.saveIndex({
      ...index,
      signedIn: false,
      reset: { profileId: id, phase: "STOPPING" },
    });
    await this.continueReset();
  }

  /**
   * Resumes a journaled reset: ends the session, then removes the database,
   * the profile files, the secrets and the profile entry, saving each phase.
   */
  async continueReset(): Promise<void> {
    let index = await this.#files.loadIndex();
    const journal = index.reset;
    if (journal === undefined) return;
    const generation = this.fence.reserve();
    try {
      await this.#close();
      if (this.fence.currentGeneration() !== generation) {
        throw SessionError.stale();
      }
      await continueResetJournal(journal, {
        removeDatabase: (id) => this.#files.removeDatabase(id),
        removeProfileFiles: (id) => this.#files.removeProfile(id),
        removeSecrets: (id) => this.#secrets.removeAll(id),
        saveJournal: async (next) => {
          index = { ...index, reset: next };
          await this.#files.saveIndex(index);
        },
        finish: async (id) => {
          const { reset: _reset, activeId, ...rest } = index;
          index = {
            ...rest,
            profiles: index.profiles.filter((profile) => profile.id !== id),
            ...(activeId === undefined || activeId === id ? {} : { activeId }),
          };
          await this.#files.saveIndex(index);
        },
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
      const index = await this.#files.loadIndex();
      await this.#files.saveIndex({ ...index, appearance: value });
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
    try {
      const index = await this.#files.loadIndex();
      const { name: _name, ...stored } = saved;
      await this.#files.saveIndex({
        ...withProfile(index, stored),
        activeId: saved.id,
        signedIn: true,
      });
    } catch (error) {
      await endQuietly(opened);
      throw error;
    }
    // The index writes awaited: an explicit action may have replaced this session.
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
