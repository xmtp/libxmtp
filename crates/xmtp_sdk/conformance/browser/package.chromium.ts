import { createInWorker } from "../../../../target/sdk-generated/typescript-wasm/package-session.gen";
import {
  Client,
  StorageAdmin,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let created = 0;
let terminated = 0;
const terminationWaiters = new Set<() => void>();
const OriginalWorker = globalThis.Worker;
globalThis.Worker = class extends OriginalWorker {
  constructor(url: string | URL, options?: WorkerOptions) {
    super(url, options);
    created++;
  }
  override terminate(): void {
    terminated++;
    super.terminate();
    for (const notify of terminationWaiters) notify();
  }
};
let admins: StorageAdmin[] = [];
let client: Client | undefined;
let pending: Promise<StorageAdmin> | undefined;
let allowCreate: (() => void) | undefined;

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
    ),
  );
  if ((await client.storage().path()) !== path) throw new Error("path changed");
}
export async function endClient(): Promise<void> {
  await client?.end();
  client = undefined;
}
