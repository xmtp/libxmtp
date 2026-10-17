import { createInWorker } from "../../../../target/sdk-generated/typescript-wasm/package-session.gen";
import {
  Client,
  StorageAdmin,
  Storage,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let created = 0;
let terminated = 0;
const terminationWaiters = new Set<() => void>();
let holdTermination = false;
let finishTermination: (() => void) | undefined;
let onTermination: (() => void) | undefined;
let failing = false;
const OriginalWorker = globalThis.Worker;
globalThis.Worker = class extends OriginalWorker {
  constructor(url: string | URL, options?: WorkerOptions) {
    super(url, options);
    created++;
    // A failing worker reports an error before its handshake.
    if (failing) queueMicrotask(() => this.dispatchEvent(new Event("error")));
  }
  override terminate(): void {
    terminated++;
    if (holdTermination) {
      holdTermination = false;
      finishTermination = () => super.terminate();
    } else super.terminate();
    onTermination?.();
    for (const notify of terminationWaiters) notify();
  }
};
let admins: StorageAdmin[] = [];
let client: Client | undefined;
let pending: Promise<StorageAdmin> | undefined;
let allowCreate: (() => void) | undefined;

/** New package workers fail at once while this is on. */
export function failWorkers(on: boolean): void {
  failing = on;
}
export function counts() {
  return { created, terminated };
}
export function waitForTermination(count: number): Promise<void> {
  if (terminated === count) return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const check = () => {
      if (terminated !== count) return;
      clearTimeout(timer);
      terminationWaiters.delete(check);
      resolve();
    };
    const timer = setTimeout(() => {
      terminationWaiters.delete(check);
      reject(
        new Error(
          `worker termination missing: expected ${count}, got ${terminated}`,
        ),
      );
    }, 5000);
    terminationWaiters.add(check);
  });
}
/**
 * Resolves when every package worker created so far has terminated. Unlike a
 * termination count, this state cannot be passed by a late termination: no
 * worker can end again until a new one starts.
 */
export function waitForIdle(): Promise<void> {
  if (terminated === created) return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const check = () => {
      if (terminated !== created) return;
      clearTimeout(timer);
      terminationWaiters.delete(check);
      resolve();
    };
    const timer = setTimeout(() => {
      terminationWaiters.delete(check);
      reject(
        new Error(
          `worker termination missing: created ${created}, terminated ${terminated}`,
        ),
      );
    }, 5000);
    terminationWaiters.add(check);
  });
}
export async function failFactory(): Promise<void> {
  try {
    await createInWorker(async () => {
      throw new Error("factory failed");
    });
  } catch (error) {
    if (!(error instanceof Error) || error.message !== "factory failed")
      throw error;
    return;
  }
  throw new Error("factory did not fail");
}
export async function openAdmins(): Promise<void> {
  admins.push(
    ...(await Promise.all([
      createInWorker((session) => StorageAdmin.open(session)),
      createInWorker((session) => StorageAdmin.open(session)),
    ])),
  );
  for (const admin of admins)
    if ((await admin.poolCapacity()) === 0)
      throw new Error("pool was not opened");
}
export async function endAdmin(): Promise<void> {
  const admin = admins.shift();
  if (!admin) throw new Error("no admin");
  await admin.end();
}
export function collectAdmins(): void {
  admins = [];
}
export async function beginDelayedCreate(): Promise<void> {
  let started!: () => void;
  const entered = new Promise<void>((resolve) => {
    started = resolve;
  });
  const gate = new Promise<void>((resolve) => {
    allowCreate = resolve;
  });
  pending = createInWorker(async (session) => {
    started();
    await gate;
    return StorageAdmin.open(session);
  });
  await entered;
}
export async function finishDelayedCreate(): Promise<void> {
  allowCreate?.();
  if (!pending) throw new Error("no pending create");
  admins.push(await pending);
  pending = undefined;
}
export async function openClient(path: string): Promise<void> {
  const identifier = `0x${Array.from(crypto.getRandomValues(new Uint8Array(20)), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  client = await createInWorker((session) =>
    Client.create(
      session,
      {
        identity: async () => ({
          identifier,
          kind: B.PublicIdentityKind.Ethereum,
        }),
        kind: async () => B.SignerKind.Eoa.new(),
        sign: async () => {
          throw new Error("registration disabled");
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
    ),
  );
  if ((await client.storage().path()) !== path) throw new Error("path changed");
}
export async function endClient(): Promise<void> {
  await client?.end();
  client = undefined;
}

export async function publicAdminRoundTrip(path: string): Promise<void> {
  const first = await Storage.admin();
  const second = await Storage.admin();
  if (!(await first.listFiles()).includes(path))
    throw new Error("public admin file missing");
  if ((await first.fileCount()) !== (await first.listFiles()).length)
    throw new Error("public admin count differs");
  if ((await first.poolCapacity()) === 0)
    throw new Error("public pool is empty");
  const data = await first.exportDb(path);
  if (!(data instanceof Uint8Array))
    throw new Error("public export must return Uint8Array");
  const padded = new Uint8Array(data.length + 16);
  padded.set(data, 8);
  const view = padded.subarray(8, 8 + data.length);
  const imported = `${path}.copy`;
  await first.importDb(imported, view);
  if (view.byteLength !== data.byteLength || view[0] !== data[0])
    throw new Error("import changed caller bytes");
  if (!(await first.fileExists(imported)))
    throw new Error("public import missing");
  if (!(await first.deleteFile(imported)))
    throw new Error("public delete missing");
  await Promise.all([first.end(), first.end()]);
  if (!(await second.fileExists(path)))
    throw new Error("first admin ended second admin");
  let closed = false;
  try {
    await first.fileCount();
  } catch (error) {
    closed = B.XmtpError.ClientClosed.instanceOf(error);
  }
  if (!closed) throw new Error("public admin did not fence after end");
  await second.clearAll();
  if ((await second.fileCount()) !== 0) throw new Error("public clear failed");
  await second.end();
}

export async function immediateReplacement(): Promise<void> {
  const first = await Storage.admin();
  const before = counts();
  let requested!: () => void;
  const request = new Promise<void>((resolve) => {
    requested = resolve;
  });
  let replacement:
    | Promise<Awaited<ReturnType<typeof Storage.admin>>>
    | undefined;
  holdTermination = true;
  onTermination = () => {
    onTermination = undefined;
    replacement = Storage.admin();
    // Handle a rejected red-control replacement without an unhandled rejection.
    void replacement.catch(() => {});
    requested();
  };
  try {
    await first.end();
    await request;
    if (created !== before.created)
      throw new Error(
        "replacement started before old worker released its locks",
      );
    finishTermination?.();
    finishTermination = undefined;
    const next = await replacement;
    if (!next || (await next.poolCapacity()) === 0)
      throw new Error("replacement did not acquire OPFS");
    await next.end();
    await waitForTermination(before.terminated + 2);
  } finally {
    onTermination = undefined;
    holdTermination = false;
    finishTermination?.();
    finishTermination = undefined;
  }
}
