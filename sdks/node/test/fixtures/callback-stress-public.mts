import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

import type * as SDK from "@xmtp/node-sdk";

import { notificationBackend } from "../notificationBackend";

const [entry, family, widthText, cyclesText] = process.argv.slice(2);
const families = [
  "credential",
  "credentialReject",
  "identity",
  "kind",
  "sign",
  "preAuthenticate",
  "event",
  "log",
];
assert.ok(
  entry && families.includes(family),
  "unknown callback family or missing public entry",
);
const callbacksPerOperation = family === "credentialReject" ? 6 : 1;
const width = Number(widthText);
const cycles = Number(cyclesText);
assert.ok(Number.isInteger(width) && width >= 1 && width <= 32);
assert.ok(Number.isInteger(cycles) && cycles >= 1 && cycles <= 20);
const sdk: typeof SDK = await import(entry);
const core = await import(
  pathToFileURL(
    join(
      dirname(fileURLToPath(entry)),
      "node_modules/@ubjs/core/dist/esm/index.js",
    ),
  ).href
);
const counts = () => ({
  rustFutureResolvers: core.uniffiRustFutureHandleCount(),
  foreignFutureTasks: core.uniffiForeignFutureHandleCount(),
});
const backend = await notificationBackend({ responseDelayMs: 5 });
const connected = await sdk.Backend.connect({ url: backend.url });
const ownerIdentity = {
  kind: "ethereum" as const,
  identifier: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
};
const reentryClient = await sdk.Client.create(
  {
    identity: async () => ownerIdentity,
    kind: async () => ({ kind: "eoa" }),
    sign: async () => {
      throw new Error("unregistered reentry client requested a signature");
    },
  },
  {
    backend: connected,
    storage: { location: "inMemory" },
    registration: { auto: false },
    deviceSync: false,
  },
);
let calls = 0;
let active = 0;
let peakActive = 0;
let heartbeats = 0;
let reentryRequests = 0;
const privateReason = "callback-stress-private-sentinel";
const timer = setInterval(() => {
  heartbeats++;
}, 1);
const ledger: Array<{ cycle: number; callbacks: number; requests: number }> =
  [];
const unhandled: unknown[] = [];
process.on("unhandledRejection", (error) => unhandled.push(error));
const invoke = async (index: number) => {
  calls++;
  active++;
  peakActive = Math.max(peakActive, active);
  try {
    const identity = {
      kind: "ethereum" as const,
      identifier: `0x${(index + 1).toString(16).padStart(40, "0")}`,
    };
    const map = await reentryClient.canMessage([identity]);
    assert.deepEqual(Array.from(map.entries()), [
      [`ethereum:${identity.identifier}`, false],
    ]);
    reentryRequests++;
    await delay(1);
  } finally {
    active--;
  }
};
const reject = (index: number): never => {
  // oxlint-disable-next-line typescript/only-throw-error -- Check a JavaScript callback rejection value.
  if (index % 3 === 0) throw null;
  // oxlint-disable-next-line typescript/only-throw-error -- Check a JavaScript callback rejection value.
  if (index % 3 === 1) throw undefined;
  throw new Error(privateReason);
};
function failure(error: unknown, code: string, retryable: boolean): void {
  assert.ok(error instanceof sdk.XmtpError);
  assert.equal(error.details.code, code);
  assert.equal(error.details.category, "callback");
  assert.equal(error.details.retryable, retryable);
  assert.ok(!JSON.stringify(error).includes(privateReason));
  assert.ok(!("cause" in error));
}
async function eventually(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 500; attempt++) {
    if (predicate()) return;
    await delay(10);
  }
  assert.fail("callback work did not settle");
}
let eventClient: SDK.Client | undefined;
let listener: bigint | undefined;
let logInstalled = false;
const joinedIds = new Set<string>();
const createdIds = new Set<string>();
let primaryFailed = false;
async function cleanup(actions: Array<() => Promise<unknown>>): Promise<void> {
  const errors: unknown[] = [];
  for (const action of actions) {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        action(),
        new Promise((_, reject) => {
          timeout = setTimeout(
            () => reject(new Error("cleanup did not settle")),
            3000,
          );
        }),
      ]);
    } catch (error) {
      errors.push(error);
    } finally {
      clearTimeout(timeout);
    }
  }
  clearInterval(timer);
  if (errors.length && !primaryFailed)
    throw new AggregateError(errors, "cleanup failed");
  if (errors.length)
    console.error(JSON.stringify({ cleanupFailures: errors.length }));
}
try {
  if (family === "event") {
    assert.ok(
      process.env.XMTP_BACKEND_URL,
      "event stress needs the recorded development backend",
    );
    eventClient = await sdk.Client.create(await sdk.generateLocalSigner(), {
      backend: { url: process.env.XMTP_BACKEND_URL },
      storage: { location: "inMemory" },
      deviceSync: false,
    });
    listener = await eventClient.startListener(
      { kinds: ["conversation.joined"], referencesOwnMessages: false },
      async (event) => {
        assert.equal(event.kind, "conversation.joined");
        assert.ok(
          !joinedIds.has(event.conversation_joined.conversationId),
          "duplicate joined event",
        );
        joinedIds.add(event.conversation_joined.conversationId);
        await invoke(calls);
      },
    );
  }
  if (family === "log") {
    await sdk.initLogging({ level: "error", structured: true });
    await sdk.setLogSink({
      log: async () => {
        await invoke(calls);
      },
    });
    logInstalled = true;
  }
  for (let cycle = 0; cycle < cycles; cycle++) {
    if (family === "event") {
      const groups = await Promise.all(
        Array.from({ length: width }, () =>
          eventClient!.conversations.createGroup([]),
        ),
      );
      for (const group of groups) createdIds.add(group.id);
      await eventually(() => calls === (cycle + 1) * width && active === 0);
      assert.deepEqual(joinedIds, createdIds);
    } else if (family === "log") {
      await Promise.all(
        Array.from({ length: width }, () =>
          assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31))),
        ),
      );
      await eventually(() => calls === (cycle + 1) * width && active === 0);
      assert.equal(peakActive, 1);
    } else {
      await Promise.all(
        Array.from({ length: width }, async (_, offset) => {
          const index = cycle * width + offset;
          let error: unknown;
          if (family === "credential" || family === "credentialReject") {
            let attempts = 0;
            error = await sdk.Client.canMessage(
              [
                {
                  kind: "ethereum",
                  identifier: `0x${(index + 10000).toString(16).padStart(40, "0")}`,
                },
              ],
              {
                url: backend.url,
                credentials: {
                  credential: async () => {
                    attempts++;
                    await invoke(index);
                    if (family === "credentialReject") return reject(index);
                    return {
                      value: "Bearer callback-stress-fixture",
                      expiresAtSeconds: BigInt(
                        Math.floor(Date.now() / 1000) + 3600,
                      ),
                    };
                  },
                },
              },
            ).catch((cause) => cause);
            assert.equal(attempts, callbacksPerOperation);
            if (family === "credentialReject") {
              failure(error, "CredentialCallbackFailed", true);
            } else {
              assert.ok(error instanceof Map);
              assert.deepEqual(Array.from(error.entries()), [
                [
                  `ethereum:0x${(index + 10000).toString(16).padStart(40, "0")}`,
                  false,
                ],
              ]);
            }
          } else {
            const identity = {
              kind: "ethereum" as const,
              identifier: `0x${(index + 10000).toString(16).padStart(40, "0")}`,
            };
            error = await sdk.Client.create(
              {
                identity: async () => {
                  if (family === "identity") {
                    await invoke(index);
                    reject(index);
                  }
                  return identity;
                },
                kind: async () => {
                  if (family === "kind") {
                    await invoke(index);
                    reject(index);
                  }
                  return { kind: "eoa" };
                },
                sign: async () => {
                  await invoke(index);
                  return reject(index);
                },
              },
              {
                backend: { url: backend.url },
                storage: { location: "inMemory" },
                deviceSync: false,
                handlers: {
                  preAuthenticate: {
                    run: async () => {
                      if (family === "preAuthenticate") {
                        await invoke(index);
                        reject(index);
                      }
                    },
                  },
                },
              },
            ).catch((cause) => cause);
            failure(
              error,
              family === "preAuthenticate" ? "CallbackFailed" : "SignerFailed",
              false,
            );
          }
        }),
      );
    }
    assert.equal(calls, (cycle + 1) * width * callbacksPerOperation);
    assert.equal(reentryRequests, calls);
    assert.equal(active, 0);
    ledger.push({ cycle, callbacks: calls, requests: reentryRequests });
  }
  assert.ok(heartbeats > 0);
  assert.equal(unhandled.length, 0, "unhandled app callback rejection");
} catch (error) {
  primaryFailed = true;
  throw error;
} finally {
  await cleanup([
    async () => {
      if (listener !== undefined) await eventClient!.stopListener(listener);
    },
    async () => {
      if (eventClient) await eventClient.end();
    },
    async () => {
      if (logInstalled) await sdk.setLogSink();
    },
    () => reentryClient.end(),
    () => backend.close(),
  ]);
}
await eventually(
  () => counts().rustFutureResolvers === 0 && counts().foreignFutureTasks === 0,
);
assert.equal(unhandled.length, 0);
console.log(
  JSON.stringify({
    result: "PASS",
    family,
    callbacksPerOperation,
    width,
    cycles,
    ledger,
    calls,
    active,
    peakActive,
    heartbeats,
    counts: counts(),
    nativeCallbackRegistrationCounts: "UNVERIFIED",
    node: process.version,
  }),
);
