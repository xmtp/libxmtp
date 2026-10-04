import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import {
  Backend,
  Client,
  decodeObjectSigner,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import { FOREIGN_METHODS } from "../../../../target/sdk-generated/typescript-wasm/wire.gen.ts";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.ts";
const { privateKeyToAccount } = await import(
  pathToFileURL(
    realpathSync(
      new URL(
        "../../../../sdks/node/node_modules/viem/_esm/accounts/privateKeyToAccount.js",
        import.meta.url,
      ),
    ),
  ).href
);

/** Fail when any string or byte array in `value` carries a secret. */
/** The retry flag of a binding error's details. */
function retryableOf(error: unknown): unknown {
  const inner: unknown =
    error !== null && typeof error === "object"
      ? Reflect.get(error, "inner")
      : undefined;
  return Array.isArray(inner) &&
    inner[0] !== null &&
    typeof inner[0] === "object"
    ? Reflect.get(inner[0], "retryable")
    : undefined;
}

function assertNoSecret(
  value: unknown,
  secrets: (string | Uint8Array)[],
  path = "options",
  seen = new Set<object>(),
): void {
  if (typeof value === "string") {
    for (const secret of secrets)
      if (typeof secret === "string" && value.includes(secret))
        throw new Error(`${path} exposes a secret`);
    return;
  }
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  const bytes =
    value instanceof ArrayBuffer
      ? new Uint8Array(value)
      : ArrayBuffer.isView(value)
        ? new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
        : undefined;
  if (bytes) {
    for (const secret of secrets)
      if (
        secret instanceof Uint8Array &&
        Buffer.from(bytes).equals(Buffer.from(secret))
      )
        throw new Error(`${path} exposes a secret key`);
    return;
  }
  for (const key of Object.getOwnPropertyNames(value))
    assertNoSecret(
      (value as Record<string, unknown>)[key],
      secrets,
      `${path}.${key}`,
      seen,
    );
}

function start(): {
  worker: Worker;
  session: MainSession;
  fatal: () => boolean;
} {
  const worker = new Worker(new URL("./bridge.worker.mts", import.meta.url), {
    execArgv: process.execArgv,
  });
  let sawFatal = false;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, transfer);
    },
    onMessage(handler) {
      worker.on("message", (message: WireMessage) => {
        if (message.t === "fatal") sawFatal = true;
        handler(message);
      });
    },
    onExit(handler) {
      worker.on("exit", handler);
      worker.on("error", handler);
    },
    terminate() {
      void worker.terminate();
    },
  };
  return {
    worker,
    session: new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH),
    fatal: () => sawFatal,
  };
}

let first = start();
try {
  await first.session.ready();
  const backend = await Backend.connect(first.session, {
    url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
    appVersion: undefined,
    credentials: undefined,
    credential: undefined,
  });
  assert.equal(backend.handle.type, "Backend");
  assert.equal(backend.handle.epoch, first.session.currentEpoch);

  const signer = first.session.callbacks.register(
    "Signer",
    { sign: () => first.session.call("__bridgeInner", []) },
    Object.keys(FOREIGN_METHODS.Signer),
  );
  assert.equal(
    await first.session.call("__bridgeReentrantSigner", [signer]),
    "inner result",
  );

  let identities = 0;
  let kinds = 0;
  await assert.rejects(
    Client.create(
      first.session,
      {
        async identity() {
          identities++;
          return {
            identifier: "0x0000000000000000000000000000000000000001",
            kind: B.PublicIdentityKind.Ethereum,
          };
        },
        async kind() {
          kinds++;
          return B.SignerKind.Eoa.new();
        },
        async sign() {
          throw new Error("unexpected sign");
        },
      },
      {
        backend: new B.BackendSource.Options({
          options: {
            url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
            appVersion: undefined,
            credentials: undefined,
            credential: undefined,
          },
        }),
        storage: {
          location: B.StorageLocation.Default.new(),
          label: undefined,
          encryptionKey: undefined,
          pool: undefined,
          singleConnection: false,
        },
        deviceSync: false,
        registration: { auto: true, nonce: undefined },
        forkRecovery: undefined,
        workers: undefined,
      },
    ),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.ok(B.XmtpError.StorageLocation.instanceOf(error));
      assert.equal(retryableOf(error), false, String(error));
      assert.ok(!B.XmtpError.StorageBusy.instanceOf(error));
      return true;
    },
  );
  assert.equal(
    identities,
    1,
    "real WASM must decode numeric PublicIdentityKind",
  );
  assert.equal(kinds, 0, "unsupported OPFS fails before signer kind");
  // A directory without OPFS fails at its deployment record, before the
  // database pool opens, so the worker lives and runs the next create.
  assert.equal(first.session.isTerminated, false);

  await assert.rejects(
    Client.create(
      first.session,
      {
        async identity() {
          return {
            identifier: "0x0000000000000000000000000000000000000001",
            kind: B.PublicIdentityKind.Ethereum,
          };
        },
        async kind() {
          return B.SignerKind.Eoa.new();
        },
        async sign() {
          throw new Error("unexpected sign");
        },
      },
      {
        backend: new B.BackendSource.Options({
          options: {
            url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
            appVersion: undefined,
            credentials: undefined,
            credential: undefined,
          },
        }),
        storage: {
          location: B.StorageLocation.Explicit.new({
            dbPath: "unsupported-opfs.db",
            attachmentsDir: "unsupported-opfs-attachments",
          }),
          label: undefined,
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
    (error: unknown) => {
      assert.ok(B.XmtpError.StorageLocation.instanceOf(error));
      assert.equal(retryableOf(error), false, String(error));
      assert.ok(!B.XmtpError.StorageBusy.instanceOf(error));
      return true;
    },
  );
  // An explicit database opens the pool first, and its failed transition
  // ends the worker.
  assert.equal(first.session.isTerminated, true);
  await first.worker.terminate();
  first = start();
  await first.session.ready();

  const account = privateKeyToAccount(`0x${randomBytes(32).toString("hex")}`);
  let liveIdentities = 0;
  let liveKinds = 0;
  const live = await Client.create(
    first.session,
    {
      async identity() {
        liveIdentities++;
        return {
          identifier: account.address,
          kind: B.PublicIdentityKind.Ethereum,
        };
      },
      async kind() {
        liveKinds++;
        return B.SignerKind.Eoa.new();
      },
      async sign(request) {
        const signed = await account.signMessage({ message: request.text });
        return B.Signature.Ecdsa.new(
          Uint8Array.from(Buffer.from(signed.slice(2), "hex")).buffer,
        );
      },
    },
    {
      backend: new B.BackendSource.Options({
        options: {
          url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
          appVersion: undefined,
          credentials: undefined,
          credential: undefined,
        },
      }),
      storage: {
        location: B.StorageLocation.InMemory.new(),
        label: undefined,
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
  assert.ok(
    liveIdentities > 0,
    "real WASM must decode numeric PublicIdentityKind",
  );
  assert.ok(liveKinds > 0);
  assert.strictEqual(live.conversations(), live.conversations());
  if (typeof global.gc === "function") {
    for (let attempt = 0; attempt < 20; attempt++) {
      global.gc();
      await new Promise<void>((resolve) => setTimeout(resolve, 5));
    }
  }
  const group = await live.conversations().createGroup([]);
  assert.equal((await group.messages()).length, 0);
  const reader = await group.messageReader();
  const abortRead = new AbortController();
  const timeout = setTimeout(() => abortRead.abort(), 10000);
  const next = reader.next({ signal: abortRead.signal });
  await group.sendText("bridge numeric message enums");
  const messages = await group.messages();
  assert.ok(messages.length > 0);
  assert.equal(typeof messages[0].kind, "number");
  assert.equal(typeof messages[0].deliveryStatus, "number");
  const streamed = await next;
  clearTimeout(timeout);
  assert.ok(streamed);
  assert.equal(typeof streamed.kind, "number");
  assert.equal(typeof streamed.deliveryStatus, "number");
  await reader.end();
  assert.equal(
    await reader.connectionState(),
    B.ConnectionState.Closed,
    "connectionState must read the live reader state",
  );
  const ending = live.end();
  // An immutable getter reads its held snapshot while the client ends, as on
  // Node (Decision 14). A call through the result fails with ClientClosed.
  const endingConversations = live.conversations();
  await assert.rejects(endingConversations.sync(), (error: unknown) => {
    assert.ok(B.XmtpError.ClientClosed.instanceOf(error));
    assert.deepEqual(error.inner[0], {
      code: "ClientClosed",
      category: B.ErrorCategory.Lifecycle,
      retryable: false,
      message: "client is closed",
      streamFailure: undefined,
    });
    return true;
  });
  await ending;

  // The Client snapshot includes options(). Its pre-authentication handler
  // is a main-thread callback, so the worker must not encode it again.
  // verifies: IDENT-073, IDENT-076
  const handledAccount = privateKeyToAccount(
    `0x${randomBytes(32).toString("hex")}`,
  );
  const order: string[] = [];
  const handled = await Client.create(
    first.session,
    {
      async identity() {
        return {
          identifier: handledAccount.address,
          kind: B.PublicIdentityKind.Ethereum,
        };
      },
      async kind() {
        return B.SignerKind.Eoa.new();
      },
      async sign(request) {
        order.push("sign");
        const signed = await handledAccount.signMessage({
          message: request.text,
        });
        return B.Signature.Ecdsa.new(
          Uint8Array.from(Buffer.from(signed.slice(2), "hex")).buffer,
        );
      },
    },
    {
      backend: new B.BackendSource.Options({
        options: {
          url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
          appVersion: undefined,
          credentials: undefined,
          credential: undefined,
        },
      }),
      storage: {
        location: B.StorageLocation.InMemory.new(),
        label: undefined,
        encryptionKey: undefined,
        pool: undefined,
        singleConnection: false,
      },
      deviceSync: false,
      allowOffline: false,
      registration: { auto: true, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
      handlers: {
        preAuthenticate: {
          async run() {
            order.push("preAuthenticate");
          },
        },
      },
    },
  );
  assert.deepEqual(order, ["preAuthenticate", "sign"]);
  // The snapshot returns the handler as a worker handle that calls back to
  // the app's handler on the main thread.
  await handled.options().handlers?.preAuthenticate?.run();
  assert.deepEqual(order, ["preAuthenticate", "sign", "preAuthenticate"]);
  await handled.end();

  // A Rust signer comes back as a worker-resident handle, like any object.
  const localKey = randomBytes(32);
  const keyed = decodeObjectSigner(
    first.session,
    await first.session.call("localSignerFromPrivateKey", [
      Uint8Array.from(localKey).buffer,
    ]),
  );
  assert.equal(
    (await keyed.identity()).identifier.toLowerCase(),
    privateKeyToAccount(`0x${localKey.toString("hex")}`).address.toLowerCase(),
  );
  const local = decodeObjectSigner(
    first.session,
    await first.session.call("generateLocalSigner", []),
  );
  // The options snapshot never returns the backend token. Scan everything
  // the page receives, because the worker copies the options to the page.
  const token = `Bearer bridge-${randomBytes(8).toString("hex")}`;
  const secretClient = await Client.create(
    first.session,
    decodeObjectSigner(
      first.session,
      await first.session.call("generateLocalSigner", []),
    ),
    {
      backend: new B.BackendSource.Options({
        options: {
          url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
          appVersion: undefined,
          credentials: undefined,
          credential: {
            name: undefined,
            value: token,
            expiresAtSeconds: 9_007_199_254_740_993n,
          },
        },
      }),
      storage: {
        location: B.StorageLocation.InMemory.new(),
        label: undefined,
        pool: undefined,
        singleConnection: false,
      },
      deviceSync: false,
      registration: { auto: true, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
    },
  );
  assertNoSecret(secretClient.options(), [token]);
  await secretClient.end();

  const localClient = await Client.create(first.session, local, {
    backend: new B.BackendSource.Options({
      options: {
        url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
        appVersion: undefined,
        credentials: undefined,
        credential: undefined,
      },
    }),
    storage: {
      location: B.StorageLocation.InMemory.new(),
      label: undefined,
      encryptionKey: undefined,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    registration: { auto: true, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
  });
  assert.equal(
    localClient.identity().identifier,
    (await local.identity()).identifier,
  );
  await localClient.end();

  console.log(
    "real WASM client, messages, reader, typed error, GC, end fence, and local signer passed",
  );
} finally {
  await first.worker.terminate();
}

const second = start();
try {
  await second.session.ready();
  const pending = second.session.call("__bridgeNever", []);
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
  await second.worker.terminate();
  await assert.rejects(pending, { code: "WorkerTerminated" });
  console.log("worker_threads termination passed");
} finally {
  await second.worker.terminate();
}
