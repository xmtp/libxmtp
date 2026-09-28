import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import type { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import { WorkerSessions } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/worker-sessions";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let worker: Worker | undefined;
const clients: Client[] = [];
let generations = 0;
let holdFatal = false;
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
        location: B.StorageLocation.Path.new(path),
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
          location: B.StorageLocation.Path.new(path),
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

/**
 * Aborts a create while its signer kind is pending. The store is open then.
 * Returns "ended" when the worker ends while it still holds the pool lock,
 * and "released" when the lock is free while the worker still runs.
 */
export async function abortCreateWhileSigning(path: string): Promise<string> {
  const current = await connection();
  const ended = () => current.isTerminated;
  const bytes = crypto.getRandomValues(new Uint8Array(20));
  const identifier = `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  const abort = new AbortController();
  let kindStarted: () => void = () => {};
  const started = new Promise<void>((resolve) => (kindStarted = resolve));
  const create = Client.create(
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
        location: B.StorageLocation.Path.new(path),
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
  abort.abort();
  await create;
  for (let index = 0; index < 100; index++) {
    if (ended()) {
      return "ended";
    }
    const held = await navigator.locks.query();
    if (!held.held?.some((lock) => lock.name?.startsWith("xmtp:")))
      return "released";
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
  return "held";
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
          location: B.StorageLocation.Path.new(path),
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
  let closed: unknown;
  try {
    client.conversations();
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
  sessions.terminate();
}
