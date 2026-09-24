import assert from "node:assert/strict";
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
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
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
  };
  return {
    worker,
    session: new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH),
    fatal: () => sawFatal,
  };
}

const first = start();
try {
  await first.session.ready();
  const backend = await Backend.connect(first.session, {
    url: "http://127.0.0.1:9450",
    appVersion: undefined,
    credentials: undefined,
  });
  assert.equal(backend.handle.type, "Backend");
  assert.equal(backend.handle.epoch, first.session.currentEpoch);

  const signer = first.session.callbacks.register("Signer", {
    sign: () => first.session.call("__bridgeInner", []),
  });
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
        backend: {
          url: "http://127.0.0.1:9450",
          appVersion: undefined,
          credentials: undefined,
        },
        storage: {
          location: B.StorageLocation.Default.new(),
          label: undefined,
          encryptionKey: undefined,
        },
        deviceSync: false,
      },
    ),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.ok(B.XmtpError.StorageLocationRequired.instanceOf(error));
      assert.equal(error.inner[0].category, B.ErrorCategory.Storage);
      assert.equal(error.inner[0].code, "StorageLocationRequired");
      return true;
    },
  );
  assert.equal(
    identities,
    1,
    "real WASM must decode numeric PublicIdentityKind",
  );
  assert.equal(kinds, 1);

  const account = privateKeyToAccount(
    "0x1111111111111111111111111111111111111111111111111111111111111111",
  );
  const live = await Client.create(
    first.session,
    {
      async identity() {
        return {
          identifier: account.address,
          kind: B.PublicIdentityKind.Ethereum,
        };
      },
      async kind() {
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
      backend: {
        url: "http://127.0.0.1:9450",
        appVersion: undefined,
        credentials: undefined,
      },
      storage: {
        location: B.StorageLocation.InMemory.new(),
        label: undefined,
        encryptionKey: undefined,
      },
      deviceSync: false,
    },
  );
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
  const ending = live.end();
  assert.throws(() => live.conversations(), { code: "clientClosed" });
  await ending;

  const pending = first.session.call("__bridgeNever", []);
  const trap = first.session.call("bridgeTestPanic", []);
  await assert.rejects(trap, { code: "workerTerminated" });
  await assert.rejects(pending, { code: "workerTerminated" });
  assert.ok(first.fatal(), "real WASM panic must send fatal");
  assert.throws(() => first.session.checkHandle(backend.handle), {
    code: "clientClosed",
  });
  console.log(
    "real WASM client, messages, reader, typed error, GC, end fence, and panic fatal passed",
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
  await assert.rejects(pending, { code: "workerTerminated" });
  console.log("worker_threads termination passed");
} finally {
  await second.worker.terminate();
}
