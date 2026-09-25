import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let worker: Worker | undefined;
let session: MainSession | undefined;
const clients: Client[] = [];

async function connection(): Promise<MainSession> {
  if (session) return session;
  worker = new Worker(new URL("./storage.bridge.worker.ts", import.meta.url), {
    type: "module",
  });
  const current = worker;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      current.postMessage(message, transfer);
    },
    onMessage(handler) {
      current.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      current.addEventListener("error", handler);
    },
    terminate() {
      current.terminate();
    },
  };
  session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  await session.ready();
  return session;
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

export async function endOne(): Promise<void> {
  const client = clients.shift();
  if (!client) throw new Error("no client to close");
  await client.end();
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
  worker?.terminate();
  worker = undefined;
  session = undefined;
}
