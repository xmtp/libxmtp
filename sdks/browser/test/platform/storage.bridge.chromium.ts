import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import {
  Client,
  StorageAdmin,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import type { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import { WorkerSessions } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/worker-sessions";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let worker: Worker | undefined;
const clients: Client[] = [];
const admins: StorageAdmin[] = [];
let generations = 0;
// The page can hold the fatal failure of a worker while it reads the
// worker's lock. The worker asks to close after its failure work ends.
let holdFatal = false;
let slowErrorMs = 0;
let closing: Promise<void> | undefined;
let markClosing: (() => void) | undefined;
const heldFailures: (() => void)[] = [];

const sessions = new WorkerSessions(
  () => {
    generations++;
    worker = new Worker(
      new URL("./storage.bridge.worker.ts", import.meta.url),
      {
        type: "module",
      },
    );
    const current = worker;
    const endpoint: WireEndpoint = {
      postMessage(message, transfer) {
        current.postMessage(message, transfer);
      },
      onMessage(handler) {
        current.addEventListener(
          "message",
          (event: MessageEvent<WireMessage>) => {
            if ("__fatalClosing" in event.data) {
              if (worker === current) markClosing?.();
              return;
            }
            const message = event.data;
            if (message.t === "error" && slowErrorMs > 0) {
              const end = performance.now() + slowErrorMs;
              while (performance.now() < end) {
                // Keep the main thread busy, as a slow CI main thread does.
              }
            }
            if (
              holdFatal &&
              (message.t === "fatal" ||
                (message.t === "error" && message.fatal))
            )
              heldFailures.push(() => handler(message));
            else handler(message);
          },
        );
      },
      onExit(handler) {
        current.addEventListener("error", handler);
      },
      terminate() {
        current.terminate();
        if (worker === current) worker = undefined;
      },
    };
    return endpoint;
  },
  PROTOCOL_VERSION,
  CONTRACT_HASH,
);

function connection(): Promise<MainSession> {
  return sessions.get();
}

export function workerGenerations(): number {
  return generations;
}

export async function openAdmins(): Promise<void> {
  const current = await connection();
  admins.push(
    ...(await Promise.all([
      StorageAdmin.open(current),
      StorageAdmin.open(current),
    ])),
  );
}

export async function adminFiles(): Promise<string[]> {
  const admin = admins.at(-1);
  if (!admin) throw new Error("no admin handle");
  const files = await admin.listFiles();
  if (files.length !== (await admin.fileCount()))
    throw new Error("admin count differs");
  if ((await admin.poolCapacity()) < files.length)
    throw new Error("admin capacity differs");
  return files;
}

export async function adminOpenFileIsBusy(path: string): Promise<void> {
  const admin = admins.at(-1);
  if (!admin) throw new Error("no admin handle");
  const before = await admin.listFiles();
  for (const call of [
    () => admin.exportDb(path),
    () => admin.deleteFile(path),
    () => admin.clearAll(),
  ]) {
    try {
      await call();
      throw new Error("admin accepted an open database");
    } catch (error) {
      if (!isStorageBusy(error)) throw error;
    }
  }
  if (JSON.stringify(before) !== JSON.stringify(await admin.listFiles()))
    throw new Error("busy admin call changed files");
}

export async function adminRoundTrip(path: string): Promise<void> {
  const admin = admins.at(-1);
  if (!admin) throw new Error("no admin handle");
  const data = await admin.exportDb(path);
  const target = "admin-round-trip.db";
  await admin.importDb(target, data);
  if (!(await admin.fileExists(target)))
    throw new Error("admin import missing");
  if (!(await admin.deleteFile(target)))
    throw new Error("admin delete missing");
  if (await admin.deleteFile(target))
    throw new Error("admin deleted absent file");
}

export async function endAdmin(): Promise<void> {
  const admin = admins.shift();
  if (!admin) throw new Error("no admin handle");
  await Promise.all([admin.end(), admin.end()]);
  try {
    await admin.fileCount();
    throw new Error("ended admin accepted a call");
  } catch (error) {
    if (codeOf(error) !== "ClientClosed") throw error;
  }
}

export function holdFailureTermination(): void {
  holdFatal = true;
  closing = new Promise<void>((resolve) => (markClosing = resolve));
}

export async function waitForFailureTermination(): Promise<void> {
  if (!closing) throw new Error("failure barrier is not armed");
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      closing,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("worker did not request termination")),
          5000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export function releaseFailureTermination(): void {
  holdFatal = false;
  for (const release of heldFailures.splice(0)) release();
}

export async function open(path: string): Promise<string> {
  const current = await connection();
  const bytes = crypto.getRandomValues(new Uint8Array(20));
  const identifier = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  const client = await Client.create(
    current,
    {
      async identity() {
        return { identifier, kind: B.PublicIdentityKind.Ethereum };
      },
      async kind() {
        return B.SignerKind.Eoa.new();
      },
      async sign() {
        throw new Error("registration is disabled");
      },
    },
    {
      backend: B.BackendSource.Options.new({
        options: {
          url: `${location.origin}/backend`,
          appVersion: undefined,
          credential: undefined,
          credentials: undefined,
        },
      }),
      storage: {
        location: B.StorageLocation.Explicit.new({
          dbPath: path,
          attachmentsDir: `${path}-attachments`,
        }),
        label: path,
        encryptionKey: undefined,
        pool: undefined,
        singleConnection: false,
      },
      deviceSync: false,
      registration: { auto: false, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
    },
  );
  clients.push(client);
  const storedPath = await client.storage().path();
  if (storedPath !== path)
    throw new Error(`SQLite path changed: ${String(storedPath)}`);
  return storedPath;
}

export async function failRegistration(path: string): Promise<unknown> {
  const current = await connection();
  const bytes = crypto.getRandomValues(new Uint8Array(20));
  const identifier = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  try {
    const client = await Client.create(
      current,
      {
        async identity() {
          return { identifier, kind: B.PublicIdentityKind.Ethereum };
        },
        async kind() {
          return B.SignerKind.Eoa.new();
        },
        async sign() {
          throw new Error("signer rejected registration");
        },
      },
      {
        backend: B.BackendSource.Options.new({
          options: {
            url: `${location.origin}/backend`,
            appVersion: undefined,
            credential: undefined,
            credentials: undefined,
          },
        }),
        storage: {
          location: B.StorageLocation.Explicit.new({
            dbPath: path,
            attachmentsDir: `${path}-attachments`,
          }),
          label: path,
          encryptionKey: undefined,
          pool: undefined,
          singleConnection: false,
        },
        deviceSync: false,
        registration: { auto: true, nonce: undefined },
        forkRecovery: undefined,
        workers: undefined,
      },
    );
    await client.end();
    return "opened";
  } catch (error) {
    return codeOf(error);
  }
}

interface PoolLock {
  name: string;
  clientId: string | undefined;
}

function heldPoolLocks(snapshot: LockManagerSnapshot): PoolLock[] {
  return (snapshot.held ?? []).flatMap((lock) =>
    lock.name?.startsWith("xmtp:")
      ? [{ name: lock.name, clientId: lock.clientId }]
      : [],
  );
}

/**
 * Aborts a create while its signer kind is pending. The store is open then,
 * so the worker must fail and keep the pool lock until it ends. The page
 * holds the fatal failure, so the session and the worker stay alive until
 * the worker asks to close after all its failure work. The page reads the
 * lock at that point. Returns "ended" when the lock of the same client was
 * held then and the delivered failure ended the worker, "released" when the
 * lock was free while the worker still ran, and "held" when the worker kept
 * the lock but did not fail. `slowMainThreadMs` blocks the page for that time
 * when the call error arrives, so the worker ends its failure work before the
 * page reads the lock.
 */
export async function abortCreateWhileSigning(
  path: string,
  slowMainThreadMs = 0,
): Promise<string> {
  const current = await connection();
  holdFailureTermination();
  const closeRequested = closing;
  if (!closeRequested) throw new Error("failure barrier is not armed");
  let create: Promise<string> | undefined;
  try {
    const bytes = crypto.getRandomValues(new Uint8Array(20));
    const identifier = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
    const abort = new AbortController();
    let kindStarted: () => void = () => {};
    const started = new Promise<void>((resolve) => (kindStarted = resolve));
    create = Client.create(
      current,
      {
        async identity() {
          return { identifier, kind: B.PublicIdentityKind.Ethereum };
        },
        kind() {
          kindStarted();
          return new Promise<never>(() => {});
        },
        async sign() {
          throw new Error("the signer kind never resolves");
        },
      },
      {
        backend: B.BackendSource.Options.new({
          options: {
            url: `${location.origin}/backend`,
            appVersion: undefined,
            credential: undefined,
            credentials: undefined,
          },
        }),
        storage: {
          location: B.StorageLocation.Explicit.new({
            dbPath: path,
            attachmentsDir: `${path}-attachments`,
          }),
          label: path,
          encryptionKey: undefined,
          pool: undefined,
          singleConnection: false,
        },
        deviceSync: false,
        registration: { auto: true, nonce: undefined },
        forkRecovery: undefined,
        workers: undefined,
      },
      { signal: abort.signal },
    ).then(
      () => "opened",
      (error: unknown) => codeOf(error),
    );
    const first = await Promise.race([started.then(() => "started"), create]);
    if (first !== "started") return String(first);
    // The create holds the one pool lock while its signer runs.
    const owners = heldPoolLocks(await navigator.locks.query());
    if (owners.length !== 1)
      throw new Error("the create does not hold exactly one pool lock");
    const [owner] = owners;
    slowErrorMs = slowMainThreadMs;
    abort.abort();
    // A fatal failure is held, so the create settles only when the product
    // failed without ending the worker.
    const settled = await Promise.race([
      closeRequested.then(() => undefined),
      create.then((result) => {
        if (result === "opened")
          throw new Error("the cancelled create returned a client");
        return result;
      }),
    ]);
    const held = heldPoolLocks(await navigator.locks.query()).some(
      (lock) => lock.name === owner.name && lock.clientId === owner.clientId,
    );
    if (!held) return "released";
    if (settled !== undefined) return "held";
  } finally {
    slowErrorMs = 0;
    releaseFailureTermination();
  }
  // The delivered failure ends the session, and the session ends the worker.
  // The cancelled create fails with the Cancelled code.
  const code = await create;
  if (code !== "Cancelled") return `failed with ${String(code)}`;
  return current.isTerminated ? "ended" : "running";
}

export async function poolFilenames(): Promise<string[]> {
  const root = await navigator.storage.getDirectory();
  const metadata = await root.getDirectoryHandle(".opfs-libxmtp-metadata");
  const pool = await metadata.getDirectoryHandle(".opaque");
  const names: string[] = [];
  for await (const handle of pool.values()) {
    if (handle.kind !== "file") continue;
    const bytes = new Uint8Array(
      await (await handle.getFile()).slice(0, 512).arrayBuffer(),
    );
    const end = bytes.indexOf(0);
    if (end > 0) names.push(new TextDecoder().decode(bytes.subarray(0, end)));
  }
  return names.sort();
}

export async function rejectBuildWithoutStoredIdentity(
  path: string,
  expectedCode: "IdentityNotFound" | "StorageBusy" = "IdentityNotFound",
): Promise<void> {
  const current = await connection();
  const bytes = crypto.getRandomValues(new Uint8Array(20));
  const identifier = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  const inbox = Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  try {
    const client = await Client.build(
      current,
      { identifier, kind: B.PublicIdentityKind.Ethereum },
      {
        backend: B.BackendSource.Options.new({
          options: {
            url: `${location.origin}/backend`,
            appVersion: undefined,
            credential: undefined,
            credentials: undefined,
          },
        }),
        storage: {
          location: B.StorageLocation.Explicit.new({
            dbPath: path,
            attachmentsDir: `${path}-attachments`,
          }),
          label: path,
          encryptionKey: undefined,
          pool: undefined,
          singleConnection: false,
        },
        deviceSync: false,
        allowOffline: false,
        registration: { auto: false, nonce: undefined },
        forkRecovery: undefined,
        workers: undefined,
        handlers: undefined,
      },
      inbox,
    );
    await client.end();
    throw new Error("build accepted a database without a stored identity");
  } catch (error) {
    const typed =
      expectedCode === "IdentityNotFound"
        ? B.XmtpError.IdentityNotFound.instanceOf(error)
        : B.XmtpError.StorageBusy.instanceOf(error);
    if (!typed || codeOf(error) !== expectedCode) throw error;
  }
}

export async function endOne(): Promise<void> {
  const client = clients.shift();
  if (!client) throw new Error("no client to close");
  await client.end();
  // An immutable getter reads its held snapshot after end, as on Node
  // (Decision 14). A call through the result fails with ClientClosed.
  const conversations = client.conversations();
  let closed: unknown;
  try {
    await conversations.sync();
  } catch (error) {
    closed = error;
  }
  if (
    closed === null ||
    typeof closed !== "object" ||
    !B.XmtpError.ClientClosed.instanceOf(closed)
  )
    throw new Error("Client.end did not raise XmtpError.ClientClosed");
  const detail = closed.inner[0];
  if (
    detail.code !== "ClientClosed" ||
    detail.category !== B.ErrorCategory.Lifecycle ||
    detail.retryable !== false ||
    detail.message !== "client is closed"
  )
    throw new Error("ClientClosed fields differ from the native binding");
}

export async function dropOne(): Promise<void> {
  if (clients.length === 0) throw new Error("no client to collect");
  await (await connection()).call("__gcArm", []);
  clients.shift();
}

export async function gcState(): Promise<{
  gcEntered: boolean;
  gcFinished: boolean;
}> {
  const value = await (await connection()).call("__gcState", []);
  if (value === null || typeof value !== "object")
    throw new TypeError("invalid GC state");
  return {
    gcEntered: Reflect.get(value, "gcEntered") === true,
    gcFinished: Reflect.get(value, "gcFinished") === true,
  };
}

export async function gcAllowClose(): Promise<void> {
  await (await connection()).call("__gcAllowClose", []);
}

export function codeOf(error: unknown): unknown {
  if (error === null || typeof error !== "object") return undefined;
  const direct = Reflect.get(error, "code");
  if (direct) return direct;
  const inner = Reflect.get(error, "inner");
  return Array.isArray(inner) &&
    inner[0] !== null &&
    typeof inner[0] === "object"
    ? Reflect.get(inner[0], "code")
    : undefined;
}

export function isStorageBusy(error: unknown): boolean {
  return (
    error !== null &&
    typeof error === "object" &&
    B.XmtpError.StorageBusy.instanceOf(error)
  );
}

export async function stop(): Promise<void> {
  while (clients.length > 0) await endOne();
  while (admins.length > 0) await endAdmin();
  sessions.terminate();
}
