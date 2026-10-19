import assert from "node:assert/strict";
import { test } from "node:test";

import { bytesToHex } from "./hex.ts";
import { type ProfilePaths, profilePaths } from "./paths.ts";
import {
  type BindRequest,
  type ClientFactory,
  type OpenRequest,
  type ProfileFiles,
  type SecretRef,
  type SecretStore,
  type SessionClient,
  SessionController,
} from "./sessionController.ts";
import { CancelledError, SessionCopy, SessionError } from "./sessionError.ts";
import { type SessionKey } from "./sessionFence.ts";
import {
  EMPTY_SESSION_INDEX,
  type BackendProfile,
  type SessionIndex,
} from "./sessionIndex.ts";
import { deferred, settle } from "./testing.ts";

const KEY_A = "07".repeat(32);
const KEY_B = "08".repeat(32);
const KEY_C = "09".repeat(32);

class Secrets implements SecretStore {
  readonly values = new Map<string, string>();
  failCredentialRemoval = false;
  credentialRemovalAttempts = 0;
  randomCalls = 0;
  writes = 0;
  failWrites = false;

  static key(profile: string, ref: SecretRef): string {
    return `${profile}.${ref}`;
  }

  get(profile: string, ref: SecretRef): string | undefined {
    return this.values.get(Secrets.key(profile, ref));
  }

  set(profile: string, ref: SecretRef, value: string): void {
    this.values.set(Secrets.key(profile, ref), value);
  }

  read(profile: string, ref: SecretRef): Promise<string | undefined> {
    return Promise.resolve(this.get(profile, ref));
  }

  write(profile: string, ref: SecretRef, value: string): Promise<void> {
    this.writes += 1;
    if (this.failWrites) return Promise.reject(new Error("keychain locked"));
    this.set(profile, ref, value);
    return Promise.resolve();
  }

  remove(profile: string, ref: SecretRef): Promise<void> {
    if (ref === "credential") this.credentialRemovalAttempts += 1;
    if (this.failCredentialRemoval && ref === "credential") {
      return Promise.reject(SessionError.message("Credential cleanup failed"));
    }
    this.values.delete(Secrets.key(profile, ref));
    return Promise.resolve();
  }

  removeAll(profile: string): Promise<void> {
    for (const key of Array.from(this.values.keys())) {
      if (key.startsWith(`${profile}.`)) this.values.delete(key);
    }
    return Promise.resolve();
  }

  randomBytes(count: number): Uint8Array {
    this.randomCalls += 1;
    return new Uint8Array(count).fill(this.randomCalls);
  }
}

class Files implements ProfileFiles {
  index: SessionIndex = EMPTY_SESSION_INDEX;
  removed: string[] = [];
  databases = new Set<string>();
  failLoad = false;
  failPrepare = false;
  failExports = false;
  failSave = false;
  exportRemovalAttempts = 0;
  writes = 0;
  onSave: (() => void) | undefined;

  prepare(): Promise<void> {
    return this.failPrepare
      ? Promise.reject(new Error("no storage"))
      : Promise.resolve();
  }

  loadIndex(): Promise<SessionIndex> {
    if (this.failLoad) {
      return Promise.reject(SessionError.message("Cannot read profile."));
    }
    return Promise.resolve(this.index);
  }

  saveIndex(index: SessionIndex): Promise<void> {
    if (this.failSave) return Promise.reject(new Error("disk full"));
    this.writes += 1;
    this.index = index;
    this.onSave?.();
    return Promise.resolve();
  }

  paths(profileId: string): Promise<ProfilePaths> {
    return Promise.resolve(profilePaths("/fixture/messenger", profileId));
  }

  databaseExists(profileId: string): Promise<boolean> {
    return Promise.resolve(this.databases.has(profileId));
  }

  removeDatabase(profileId: string): Promise<void> {
    this.databases.delete(profileId);
    this.removed.push("database");
    return Promise.resolve();
  }

  removeExports(): Promise<void> {
    this.exportRemovalAttempts += 1;
    if (this.failExports) {
      return Promise.reject(SessionError.message("Export cleanup failed"));
    }
    this.removed.push("exports");
    return Promise.resolve();
  }

  removeProfile(): Promise<void> {
    this.removed.push("profile");
    return Promise.resolve();
  }
}

class Client implements SessionClient {
  readonly inboxId: string;
  readonly identity: string;
  ended = false;
  endCalls = 0;
  cleaned = false;
  endedAfterCleanup = false;
  holdEnd = false;
  endGate: (() => void) | undefined;

  constructor(inboxId = "inbox", identity = "0xidentity") {
    this.inboxId = inboxId;
    this.identity = identity;
  }

  async end(): Promise<void> {
    this.endCalls += 1;
    if (this.holdEnd) {
      await new Promise<void>((resolve) => {
        this.endGate = resolve;
      });
    }
    this.ended = true;
    this.endedAfterCleanup = this.cleaned;
  }
}

class Factory implements ClientFactory<Client> {
  client = new Client();
  opens = 0;
  failNext = false;
  requests: OpenRequest[] = [];
  pending:
    | { resolve(client: Client): void; reject(error: unknown): void }
    | undefined;
  pause = false;
  clientFor: ((request: OpenRequest) => Client) | undefined;

  open(request: OpenRequest): Promise<Client> {
    this.opens += 1;
    this.requests.push(request);
    if (this.failNext) {
      this.failNext = false;
      return Promise.reject(SessionError.message("Factory offline"));
    }
    if (this.pause) {
      return new Promise((resolve, reject) => {
        this.pending = { resolve, reject };
      });
    }
    return Promise.resolve(this.clientFor?.(request) ?? this.client);
  }
}

interface Session {
  readonly key: SessionKey;
  readonly profile: BindRequest<Client>["profile"];
  readonly request: BindRequest<Client>;
}

function controller(
  options: {
    files?: Files;
    secrets?: Secrets;
    factory?: Factory;
  } = {},
) {
  const files = options.files ?? new Files();
  const secrets = options.secrets ?? new Secrets();
  const factory = options.factory ?? new Factory();
  let ids = 0;
  const session = new SessionController<Client, Session>({
    files,
    secrets,
    factory,
    newProfileId: () => {
      ids += 1;
      return `profile-${String(ids)}`;
    },
    bind: (request) => ({
      key: request.key,
      profile: request.profile,
      request,
    }),
  });
  return { files, secrets, factory, session };
}

function active(session: SessionController<Client, Session>): Session {
  const state = session.getState();
  assert.equal(state.kind, "active");
  return state.session;
}

function savedProfile(
  files: Files,
  url: string,
  established = true,
): BackendProfile {
  const profile: BackendProfile = established
    ? { id: "saved", url, inboxId: "saved-inbox", identity: "saved-identity" }
    : { id: "saved", url };
  files.index = { ...files.index, profiles: [profile], activeId: profile.id };
  return profile;
}

function saveKeys(secrets: Secrets, profile: string, skip?: SecretRef): void {
  const keys: readonly (readonly [SecretRef, string])[] = [
    ["wallet", KEY_A],
    ["database-key", KEY_B],
    ["preferences-key", KEY_C],
  ];
  for (const [ref, value] of keys) {
    if (ref !== skip) secrets.set(profile, ref, value);
  }
}

/** A device with one saved profile for `url`, its keys and its database. */
function savedSession(
  options: {
    url?: string;
    signedIn?: boolean;
    database?: boolean;
    skip?: SecretRef;
  } = {},
) {
  const files = new Files();
  const secrets = new Secrets();
  const profile = savedProfile(files, options.url ?? "https://example.com");
  files.index = { ...files.index, signedIn: options.signedIn ?? true };
  if (options.database ?? true) files.databases.add(profile.id);
  saveKeys(secrets, profile.id, options.skip);
  return { files, secrets, profile };
}

function hexOf(bytes: Uint8Array | undefined): string {
  return bytesToHex(bytes ?? new Uint8Array());
}

test("first join creates three secrets, stores the token and records the inbox", async () => {
  const { files, secrets, factory, session } = controller();
  await session.connect({
    url: "http://localhost:5050",
    token: "token",
    name: "Local",
  });
  const { profile } = active(session);
  assert.equal(profile.url, "http://localhost:5050");
  assert.equal(profile.inboxId, "inbox");
  assert.equal(profile.name, "Local");
  for (const ref of ["wallet", "database-key", "preferences-key"] as const) {
    assert.match(secrets.get(profile.id, ref) ?? "", /^[0-9a-f]{64}$/u, ref);
  }
  assert.equal(secrets.get(profile.id, "credential"), "token");
  const request = factory.requests.at(0);
  assert.equal(request?.profile.inboxId, undefined);
  assert.equal(hexOf(request?.wallet), secrets.get(profile.id, "wallet"));
  assert.equal(await request?.credential?.(), "token");
  assert.deepEqual(files.index.profiles, [
    {
      id: profile.id,
      url: "http://localhost:5050",
      inboxId: "inbox",
      identity: "0xidentity",
    },
  ]);
  assert.equal(files.index.activeId, profile.id);
  assert.ok(files.index.signedIn);
  // The name lives in preferences, not in the index.
  assert.ok(!JSON.stringify(files.index).includes("Local"));
});

test("an empty token removes a stored one and attaches no credential source", async () => {
  const { files, secrets, factory, session } = controller();
  await session.connect({ url: "https://example.com", token: "token" });
  const { profile } = active(session);
  files.databases.add(profile.id);
  await session.connect({ url: "https://example.com", token: "" });
  assert.equal(secrets.get(profile.id, "credential"), undefined);
  assert.equal(factory.requests.at(-1)?.credential, undefined);
});

test("the credential source stops for a replaced session", async () => {
  const { secrets, factory, session } = controller();
  await session.connect({ url: "https://example.com", token: "token" });
  const first = active(session).profile;
  const credential = factory.requests.at(0)?.credential;
  assert.ok(credential !== undefined);
  await session.connect({ url: "https://other.example", token: "other" });
  // The first server's token is still stored; only the fence stops it.
  assert.equal(secrets.get(first.id, "credential"), "token");
  await assert.rejects(
    credential(),
    (error: unknown) => error instanceof SessionError && error.kind === "stale",
  );
});

test("sign out stops session work, then ends the client and keeps the local identity", async () => {
  const { files, secrets, factory, session } = controller();
  await session.connect({ url: "http://localhost:5050", token: "token" });
  const { profile, request } = active(session);
  request.scope.addCleanup(() => {
    factory.client.cleaned = true;
  });
  assert.ok(request.isCurrent());
  await session.signOut();
  assert.ok(factory.client.ended);
  assert.ok(factory.client.endedAfterCleanup);
  assert.equal(secrets.get(profile.id, "credential"), undefined);
  assert.ok(secrets.get(profile.id, "wallet") !== undefined);
  assert.ok(secrets.get(profile.id, "database-key") !== undefined);
  assert.deepEqual(files.removed, ["exports"]);
  assert.ok(!files.index.signedIn);
  assert.equal(session.getState().kind, "signedOut");
  assert.ok(!request.isCurrent());
});

test("a concurrent reset waits for sign-out teardown", async () => {
  const { files, secrets, factory, session } = controller();
  await session.connect({ url: "http://localhost:5050", token: "token" });
  const { profile, request } = active(session);
  files.databases.add(profile.id);
  const cleanup = deferred();
  factory.client.holdEnd = true;
  request.scope.addCleanup(async () => {
    await cleanup.promise;
    factory.client.cleaned = true;
  });
  const signingOut = session.signOut();
  await settle();
  const generation = session.fence.currentGeneration();
  const resetting = session.deleteLocalData();
  await settle();
  assert.ok(session.fence.currentGeneration() > generation);
  assert.deepEqual(files.removed, [], "held in a scope cleanup");
  assert.equal(factory.client.endCalls, 0, "client.end waits for the scope");
  cleanup.resolve();
  await settle();
  assert.ok(factory.client.endGate !== undefined, "client.end is running");
  assert.deepEqual(files.removed, [], "held in client.end");
  assert.ok(secrets.get(profile.id, "wallet") !== undefined);
  factory.client.endGate?.();
  await signingOut;
  await resetting;
  assert.equal(factory.client.endCalls, 1);
  assert.ok(factory.client.endedAfterCleanup);
  assert.deepEqual(files.removed, ["database", "profile"]);
  assert.equal(secrets.values.size, 0);
  assert.equal(session.getState().kind, "signedOut");
  assert.deepEqual(files.index.profiles, []);
});

test("an interrupted reset resumes from DATABASE_REMOVED", async () => {
  const files = new Files();
  const secrets = new Secrets();
  const profile = { id: "p", url: "https://example.com" };
  files.index = {
    ...EMPTY_SESSION_INDEX,
    profiles: [profile, { id: "other", url: "https://other.example" }],
    activeId: profile.id,
    reset: { profileId: profile.id, phase: "DATABASE_REMOVED" },
  };
  secrets.set("p", "wallet", KEY_A);
  secrets.set("other", "wallet", KEY_A);
  const { session } = controller({ files, secrets });
  await session.start();
  assert.equal(session.getState().kind, "resetIncomplete");
  await session.continueReset();
  assert.equal(files.index.reset, undefined);
  assert.deepEqual(files.index.profiles, [
    { id: "other", url: "https://other.example" },
  ]);
  assert.equal(files.index.activeId, undefined);
  assert.equal(secrets.get("p", "wallet"), undefined);
  assert.equal(secrets.get("other", "wallet"), KEY_A);
  // The saved phase says the database is already gone.
  assert.ok(!files.removed.includes("database"));
  assert.ok(files.removed.includes("profile"));
  assert.equal(session.getState().kind, "signedOut");
});

test("a failed reset step keeps the journal and reports resetIncomplete", async () => {
  const { files, session } = controller();
  await session.connect({ url: "https://example.com", token: "" });
  files.removeProfile = () => Promise.reject(new Error("busy"));
  await assert.rejects(session.deleteLocalData(), /busy/u);
  assert.equal(session.getState().kind, "resetIncomplete");
  assert.equal(files.index.reset?.phase, "DATABASE_REMOVED");
  await assert.rejects(
    session.connect({ url: "https://example.com", token: "" }),
    (error: unknown) =>
      error instanceof Error && error.message === SessionCopy.finishReset,
  );
});

for (const missing of ["wallet", "database-key"] as const) {
  test(`restore never replaces a missing ${missing}`, async () => {
    const { files, secrets } = savedSession({ skip: missing });
    const before = new Map(secrets.values);
    const { factory, session } = controller({ files, secrets });
    await session.start();
    assert.equal(session.getState().kind, "restoreFailed");
    assert.deepEqual(secrets.values, before);
    assert.equal(factory.opens, 0);
  });
}

test("a secret that is not 64 hex characters counts as missing", async () => {
  const { files, secrets, profile } = savedSession();
  secrets.set(profile.id, "wallet", "07".repeat(31));
  const { factory, session } = controller({ files, secrets });
  await session.start();
  assert.equal(session.getState().kind, "restoreFailed");
  assert.equal(factory.opens, 0);
});

test("restore reopens an established profile offline with saved keys", async () => {
  const { files, secrets } = savedSession();
  const { factory, session } = controller({ files, secrets });
  factory.client = new Client("saved-inbox", "saved-identity");
  await session.start();
  assert.equal(active(session).profile.inboxId, "saved-inbox");
  const request = factory.requests.at(0);
  assert.equal(request?.profile.inboxId, "saved-inbox");
  assert.equal(request?.profile.identity, "saved-identity");
  assert.equal(hexOf(request?.databaseKey), KEY_B);
  assert.equal(request?.credential, undefined);
  assert.equal(secrets.writes, 0);
  // A second automatic restore while active does nothing.
  await session.restore();
  assert.equal(factory.opens, 1);
});

test("an index read failure before reservation allows a retry", async () => {
  const { files, session } = controller();
  await session.start();
  files.failLoad = true;
  await assert.rejects(
    session.connect({ url: "https://example.com", token: "" }),
    /Cannot read profile/u,
  );
  assert.equal(session.fence.currentGeneration(), 0);
  assert.equal(session.getState().kind, "signedOut");
  files.failLoad = false;
  await session.connect({ url: "https://example.com", token: "" });
  assert.ok(session.client !== undefined);
});

test("a late open after a newer reservation ends its client and changes nothing", async () => {
  const { files, factory, session } = controller();
  await session.start();
  factory.pause = true;
  const joining = session.connect({ url: "https://example.com", token: "" });
  await settle();
  assert.ok(factory.pending !== undefined);
  session.fence.reserve();
  const before = session.getState();
  const index = files.index;
  const late = new Client("late");
  factory.pending?.resolve(late);
  await assert.rejects(
    joining,
    (error: unknown) => error instanceof SessionError && error.kind === "stale",
  );
  assert.ok(late.ended);
  assert.equal(session.getState(), before);
  assert.equal(session.client, undefined);
  // A stale open never records its inbox or signs in.
  assert.equal(files.index, index);
  assert.ok(!files.index.signedIn);
});

test("a cancelled join ends a client that opens too late", async () => {
  const { files, factory, session } = controller();
  await session.start();
  factory.pause = true;
  const abort = new AbortController();
  const joining = session.connect({
    url: "https://example.com",
    token: "",
    signal: abort.signal,
  });
  await settle();
  assert.ok(factory.pending !== undefined);
  abort.abort();
  const late = new Client("late");
  factory.pending?.resolve(late);
  await assert.rejects(joining, CancelledError);
  assert.ok(late.ended);
  assert.equal(session.getState().kind, "signedOut");
  assert.ok(!files.index.signedIn);
  // An aborted signal stops before any SDK call.
  const opens = factory.opens;
  await assert.rejects(
    session.connect({
      url: "https://example.com",
      token: "",
      signal: abort.signal,
    }),
    CancelledError,
  );
  assert.equal(factory.opens, opens);
});

test("a failed connect with a missing secret retries with an explicit reservation", async () => {
  const { files, secrets, profile } = savedSession({
    signedIn: false,
    skip: "wallet",
  });
  const { factory, session } = controller({ files, secrets });
  await session.start();
  await assert.rejects(
    session.connect({ url: profile.url, token: "" }),
    (error: unknown) =>
      error instanceof SessionError && error.kind === "missingSecret",
  );
  assert.equal(session.getState().kind, "restoreFailed");
  assert.ok(!files.index.signedIn);
  secrets.set(profile.id, "wallet", KEY_A);
  await session.retryRestore();
  assert.ok(session.client !== undefined);
  assert.ok(files.index.signedIn);
  assert.equal(files.index.activeId, profile.id);
  assert.equal(factory.opens, 1);
});

test("an established profile without its database cannot rejoin silently", async () => {
  // Its keys survive, so only the database check stops the open.
  const { files, secrets, profile } = savedSession({
    signedIn: false,
    database: false,
  });
  const { factory, session } = controller({ files, secrets });
  await assert.rejects(
    session.connect({ url: profile.url, token: "" }),
    (error: unknown) =>
      error instanceof Error && error.message === SessionCopy.missingSecret,
  );
  assert.equal(session.getState().kind, "restoreFailed");
  assert.equal(factory.opens, 0);
});

test("an index read failure ends the uncommitted client", async () => {
  const files = new Files();
  const { factory, session } = controller({ files });
  // The index becomes unreadable after the SDK opened.
  factory.clientFor = () => {
    files.failLoad = true;
    return factory.client;
  };
  await assert.rejects(
    session.connect({ url: "https://example.com", token: "" }),
    /Cannot read profile/u,
  );
  assert.equal(factory.opens, 1);
  assert.ok(factory.client.ended);
  assert.equal(session.client, undefined);
  assert.ok(!files.index.signedIn);
});

test("an incomplete profile retries with the same wallet", async () => {
  const { files, secrets, factory, session } = controller();
  factory.failNext = true;
  await assert.rejects(
    session.connect({ url: "https://example.com", token: "" }),
    /Factory offline/u,
  );
  const saved = files.index.profiles.at(0);
  assert.ok(saved !== undefined);
  const savedSecrets = new Map(secrets.values);
  await session.connect({ url: "https://example.com", token: "" });
  assert.deepEqual(
    factory.requests.map((request) => request.profile.id),
    [saved.id, saved.id],
  );
  assert.equal(
    hexOf(factory.requests.at(0)?.wallet),
    hexOf(factory.requests.at(1)?.wallet),
  );
  assert.deepEqual(secrets.values, savedSecrets);
  assert.equal(files.index.profiles.length, 1);
  assert.ok(files.index.signedIn);
  assert.ok(session.client !== undefined);
});

for (const failure of ["credential", "exports"] as const) {
  test(`sign-out cleanup failure (${failure}) still ends the session`, async () => {
    const { files, secrets, factory, session } = controller();
    await session.connect({ url: "https://example.com", token: "token" });
    const { profile } = active(session);
    secrets.failCredentialRemoval = failure === "credential";
    files.failExports = failure === "exports";
    const expected =
      failure === "exports"
        ? "Export cleanup failed"
        : "Credential cleanup failed";
    await assert.rejects(
      session.signOut(),
      (error: unknown) => error instanceof Error && error.message === expected,
    );
    assert.equal(session.getState().kind, "signedOut");
    assert.equal(session.client, undefined);
    assert.ok(factory.client.ended);
    assert.ok(!files.index.signedIn);
    assert.ok(secrets.get(profile.id, "wallet") !== undefined);
    assert.equal(secrets.credentialRemovalAttempts, 1);
    assert.equal(files.exportRemovalAttempts, 1);
    assert.equal(
      secrets.get(profile.id, "credential") === undefined,
      !secrets.failCredentialRemoval,
    );
    assert.equal(files.removed.includes("exports"), !files.failExports);
  });
}

test("a repaired unsigned index returns to Join without writing", async () => {
  const files = new Files();
  files.index = { ...EMPTY_SESSION_INDEX, appearance: "dark" };
  files.failLoad = true;
  const secrets = new Secrets();
  secrets.failWrites = true;
  const { factory, session } = controller({ files, secrets });
  await session.start();
  assert.equal(session.getState().kind, "restoreFailed");
  files.failLoad = false;
  await session.retryRestore();
  assert.equal(session.getState().kind, "signedOut");
  assert.equal(files.index.appearance, "dark");
  assert.equal(files.writes, 0);
  assert.equal(factory.opens, 0);
  assert.equal(secrets.writes, 0);
});

for (const reset of [false, true]) {
  test(`retry keeps a signed-in failure and a reset journal (reset: ${String(reset)})`, async () => {
    const files = new Files();
    files.failLoad = true;
    const { factory, session } = controller({ files });
    await session.start();
    files.failLoad = false;
    files.index = {
      ...EMPTY_SESSION_INDEX,
      signedIn: !reset,
      ...(reset
        ? { reset: { profileId: "x", phase: "STOPPING" as const } }
        : {}),
    };
    await session.retryRestore();
    assert.equal(
      session.getState().kind,
      reset ? "resetIncomplete" : "restoreFailed",
    );
    assert.equal(files.writes, 0);
    assert.equal(factory.opens, 0);
  });
}

test("an equivalent legacy URL reuses its identity", async () => {
  const stored = "https://EXAMPLE.COM:443/API/v1/";
  const { files, secrets, profile } = savedSession({
    url: stored,
    signedIn: false,
  });
  const original = new Map(secrets.values);
  const { factory, session } = controller({ files, secrets });
  factory.client = new Client("saved-inbox", "saved-identity");
  await session.connect({ url: "https://example.com/API/v1", token: "" });
  const opened = active(session).profile;
  assert.equal(opened.id, profile.id);
  assert.equal(opened.url, stored);
  assert.equal(opened.inboxId, "saved-inbox");
  assert.equal(files.index.profiles.length, 1);
  assert.equal(files.index.activeId, profile.id);
  const request = factory.requests.at(0);
  assert.equal(hexOf(request?.wallet), KEY_A);
  assert.equal(hexOf(request?.databaseKey), KEY_B);
  assert.equal(request?.paths.root, "/fixture/messenger/profiles/saved");
  assert.deepEqual(secrets.values, original);
  assert.equal(secrets.randomCalls, 0);
  assert.deepEqual(files.removed, []);
});

test("a different endpoint creates a separate identity", async () => {
  const input = "https://example.com/api";
  const { files, secrets, profile } = savedSession({
    url: "https://EXAMPLE.COM:443/API",
    signedIn: false,
  });
  const { factory, session } = controller({ files, secrets });
  factory.clientFor = (request) =>
    request.profile.id === profile.id
      ? new Client("saved-inbox")
      : new Client("new-inbox");
  await session.connect({ url: input, token: "" });
  const opened = active(session).profile;
  assert.notEqual(opened.id, profile.id);
  assert.equal(opened.url, input);
  assert.equal(opened.inboxId, "new-inbox");
  assert.equal(files.index.profiles.length, 2);
  assert.ok(
    files.index.profiles.some(
      (entry) => entry.id === profile.id && entry.inboxId === "saved-inbox",
    ),
  );
  assert.equal(secrets.get(profile.id, "wallet"), KEY_A);
  assert.deepEqual(files.removed, []);
});

test("rejoin after sign out reopens the same inbox; delete local data starts over", async () => {
  const { files, factory, session } = controller();
  factory.clientFor = (request) =>
    new Client(request.profile.inboxId ?? `inbox-for-${request.profile.id}`);
  await session.connect({ url: "http://127.0.0.1:5150", token: "" });
  const first = active(session).profile;
  files.databases.add(first.id);
  await session.signOut();
  await session.connect({ url: "http://127.0.0.1:5150", token: "" });
  const again = active(session).profile;
  assert.equal(again.id, first.id);
  assert.equal(again.inboxId, first.inboxId);
  assert.equal(factory.requests.at(-1)?.profile.inboxId, first.inboxId);
  await session.deleteLocalData();
  await session.connect({ url: "http://127.0.0.1:5150", token: "" });
  const fresh = active(session).profile;
  assert.notEqual(fresh.id, first.id);
  assert.notEqual(fresh.inboxId, first.inboxId);
});

test("a bind failure ends the client and reports the error", async () => {
  const files = new Files();
  const factory = new Factory();
  const session = new SessionController<Client, never>({
    files,
    secrets: new Secrets(),
    factory,
    bind: () => {
      throw new Error("bind failed");
    },
  });
  await assert.rejects(
    session.connect({ url: "https://example.com", token: "" }),
    /bind failed/u,
  );
  assert.ok(factory.client.ended);
  assert.equal(session.getState().kind, "signedOut");
});

test("start reports unavailable storage and retries; appearance is global", async () => {
  const files = new Files();
  files.failPrepare = true;
  files.index = { ...EMPTY_SESSION_INDEX, appearance: "light" };
  const { session } = controller({ files });
  assert.equal(session.getState().kind, "starting");
  await session.start();
  assert.equal(session.getState().kind, "storageFailed");
  files.failPrepare = false;
  await session.start();
  assert.equal(session.getState().kind, "signedOut");
  assert.equal(session.appearance.getState(), "light");
  await session.setAppearance("dark");
  assert.equal(files.index.appearance, "dark");
  assert.equal(session.appearance.getState(), "dark");
  files.failSave = true;
  await assert.rejects(
    session.setAppearance("system"),
    (error: unknown) =>
      error instanceof Error && error.message === SessionCopy.appearanceFailed,
  );
  assert.equal(session.appearance.getState(), "dark");
});

test("two start calls share one launch without a Join flash", async () => {
  const { files, secrets } = savedSession();
  const { factory, session } = controller({ files, secrets });
  const seen: string[] = [];
  session.subscribe(() => seen.push(session.getState().kind));
  await Promise.all([session.start(), session.start()]);
  assert.equal(session.getState().kind, "active");
  assert.ok(!seen.includes("signedOut"), seen.join(","));
  assert.equal(factory.opens, 1);
});

test("restore with a missing active profile or database fails visibly", async () => {
  const files = new Files();
  files.index = { ...EMPTY_SESSION_INDEX, signedIn: true, activeId: "gone" };
  const first = controller({ files });
  await first.session.start();
  assert.equal(first.session.getState().kind, "restoreFailed");
  const other = savedSession({ database: false });
  const second = controller({ files: other.files, secrets: other.secrets });
  await second.session.start();
  assert.equal(second.session.getState().kind, "restoreFailed");
  assert.equal(second.factory.opens, 0);
});
