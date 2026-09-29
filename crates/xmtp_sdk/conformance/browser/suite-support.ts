// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { Backend } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

export function expect(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

export function equal(
  actual: unknown,
  expected: unknown,
  message: string,
): void {
  if (actual !== expected)
    throw new Error(`${message}: ${String(actual)} != ${String(expected)}`);
}

export function connection(hash = CONTRACT_HASH): {
  session: MainSession;
  worker: Worker;
} {
  const worker = new Worker(new URL("./suite.worker.ts", import.meta.url), {
    type: "module",
  });
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  return { worker, session: new MainSession(endpoint, PROTOCOL_VERSION, hash) };
}

export function signer(
  session: MainSession,
  reenter = false,
  backendURL?: string,
): {
  identity: () => Promise<B.PublicIdentity>;
  kind: () => Promise<B.SignerKind>;
  sign: (request: { text: string }) => Promise<B.Signature>;
  didReenter: () => boolean;
} {
  const account = privateKeyToAccount(generatePrivateKey());
  let reentered = false;
  return {
    async identity() {
      return {
        identifier: account.address.toLowerCase(),
        kind: B.PublicIdentityKind.Ethereum,
      };
    },
    async kind() {
      return B.SignerKind.Eoa.new();
    },
    async sign(request) {
      if (reenter) {
        if (!backendURL) throw new Error("missing backend for reentry");
        const backend = await Backend.connect(session, {
          url: backendURL,
          appVersion: undefined,
          credentials: undefined,
          credential: undefined,
        });
        equal(
          backend.handle.type,
          "Backend",
          "signer callback could not call the SDK worker",
        );
        reentered = true;
      }
      const signed = await account.signMessage({ message: request.text });
      return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
    },
    didReenter: () => reentered,
  };
}

export function options(
  path: string,
  backendURL: string,
  auto = true,
): B.ClientOptions {
  return {
    backend: B.BackendSource.Options.new({
      options: {
        url: backendURL,
        appVersion: undefined,
        credential: undefined,
        credentials: undefined,
      },
    }),
    storage: {
      location: B.StorageLocation.Path.new(path),
      label: path,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    allowOffline: false,
    registration: { auto, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
  };
}

export async function checkError(
  action: () => Promise<unknown>,
  test: (error: Error) => boolean,
  message: string,
): Promise<void> {
  try {
    await action();
  } catch (error) {
    if (error instanceof Error && test(error)) return;
    throw error;
  }
  throw new Error(message);
}

export async function checkRejectedPromise(
  action: () => Promise<unknown>,
  label: string,
): Promise<void> {
  let result: Promise<unknown>;
  try {
    result = action();
  } catch (error) {
    throw new Error(`${label} threw synchronously`, { cause: error });
  }
  expect(result instanceof Promise, `${label} did not return a promise`);
  await checkError(
    () => result,
    (error) =>
      B.XmtpError.ClientClosed.instanceOf(error) &&
      error.inner[0].code === "ClientClosed",
    `${label} did not reject with ClientClosed`,
  );
}
