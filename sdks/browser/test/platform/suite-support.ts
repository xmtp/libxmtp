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
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { Client as ProxyClient } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { wrapClient } from "../../../../target/sdk-generated/typescript-wasm/public-client.gen";
import {
  currentProjection,
  lowerPublicIdentity,
  lowerSigner,
  publicError,
} from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/public/client";

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

export function connection(): {
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
  return {
    worker,
    session: new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH),
  };
}

// The transport tests create clients in their own worker session. The public
// layer then wraps the session's worker proxy, as it wraps the package's.
export type Created = { client: sdk.Client; proxy: ProxyClient };

export async function create(
  session: MainSession,
  signer: sdk.Signer,
  options: sdk.ClientOptions,
): Promise<Created> {
  const projection = currentProjection();
  const proxy = await ProxyClient.create(
    session,
    lowerSigner(signer, projection),
    hostOptions(options, projection),
  ).catch((error: unknown) => {
    throw publicError(error);
  });
  return { client: publicClient(wrapClient(proxy)), proxy };
}

export async function build(
  session: MainSession,
  identity: sdk.PublicIdentity,
  options: sdk.ClientOptions,
  inboxId?: sdk.InboxId,
): Promise<Created> {
  const projection = currentProjection();
  const proxy = await ProxyClient.build(
    session,
    lowerPublicIdentity(identity, projection),
    hostOptions(options, projection),
    inboxId,
  ).catch((error: unknown) => {
    throw publicError(error);
  });
  return { client: publicClient(wrapClient(proxy)), proxy };
}

export function signer(): sdk.Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { identifier: account.address.toLowerCase(), kind: "ethereum" };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  };
}

export function options(
  path: string,
  backendURL: string,
  auto = true,
): sdk.ClientOptions {
  return {
    backend: { url: backendURL },
    storage: {
      location: { dbPath: path, attachmentsDir: `${path}-attachments` },
      label: path,
      singleConnection: false,
    },
    deviceSync: false,
    allowOffline: false,
    registration: { auto },
  };
}
