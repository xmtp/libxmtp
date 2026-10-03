import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

import type * as SDK from "@xmtp/node-sdk";

import { notificationBackend } from "../notificationBackend";

const [entry, route, auth, widthText, cyclesText] = process.argv.slice(2);
assert.ok(entry, "missing installed public entry URL");
assert.ok(["static", "instance", "inbox"].includes(route), "unknown route");
assert.ok(["yes", "no"].includes(auth), "unknown credential mode");
const width = Number(widthText);
const cycles = Number(cyclesText);
assert.ok(Number.isInteger(width) && width >= 1 && width <= 32);
assert.ok(Number.isInteger(cycles) && cycles >= 1 && cycles <= 20);
const sdk: typeof SDK = await import(entry);
const packageRoot = dirname(fileURLToPath(entry));
const core = await import(
  pathToFileURL(join(packageRoot, "node_modules/@ubjs/core/dist/esm/index.js"))
    .href
);
const counts = () => ({
  rustFutureResolvers: core.uniffiRustFutureHandleCount(),
  foreignFutureTasks: core.uniffiForeignFutureHandleCount(),
});
assert.deepEqual(counts(), { rustFutureResolvers: 0, foreignFutureTasks: 0 });
const backend = await notificationBackend({ responseDelayMs: 5 });
let credentialCalls = 0;
let signerIdentityCalls = 0;
let heartbeats = 0;
let pending = 0;
let peakPending = 0;
let heartbeatWhilePending = 0;
const timer = setInterval(() => {
  heartbeats++;
  if (pending) heartbeatWhilePending++;
}, 1);
const options = {
  url: backend.url,
  credentials:
    auth === "yes"
      ? {
          credential: async () => {
            credentialCalls++;
            await delay(1);
            return {
              value: "Bearer concurrency-fixture",
              expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
            };
          },
        }
      : undefined,
};
const connected = await sdk.Backend.connect(options);
let client: SDK.Client | undefined;
const rows: Array<{ cycle: number; requests: number; maps: number }> = [];
let succeeded = false;
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
  if (route !== "static") {
    client = await sdk.Client.create(
      {
        identity: async () => {
          signerIdentityCalls++;
          await delay(1);
          return {
            kind: "ethereum",
            identifier: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          };
        },
        kind: async () => ({ kind: "eoa" }),
        sign: async () => {
          throw new Error(
            "an unregistered lookup must not ask for a signature",
          );
        },
      },
      {
        backend: connected,
        storage: { location: "inMemory" },
        registration: { auto: false },
        deviceSync: false,
      },
    );
    assert.equal(signerIdentityCalls, 1);
  }
  const before = backend.requests.filter(({ path }) =>
    path.endsWith("/GetInboxIds"),
  ).length;
  for (let cycle = 0; cycle < cycles; cycle++) {
    const identities = Array.from({ length: width }, (_, index) => ({
      kind: "ethereum" as const,
      identifier: `0x${(cycle * width + index + 1).toString(16).padStart(40, "0")}`,
    }));
    const results = await Promise.all(
      identities.map(async (identity) => {
        pending++;
        peakPending = Math.max(peakPending, pending);
        try {
          return route === "static"
            ? await sdk.Client.canMessage([identity], connected)
            : route === "instance"
              ? await client!.canMessage([identity])
              : await client!.inboxIdFor(identity);
        } finally {
          pending--;
        }
      }),
    );
    for (let index = 0; index < width; index++) {
      const result = results[index];
      if (route === "inbox") assert.equal(result, undefined);
      else {
        assert.ok(result instanceof Map);
        assert.deepEqual(Array.from(result.entries()), [
          [`ethereum:${identities[index].identifier}`, false],
        ]);
      }
    }
    const received =
      backend.requests.filter(({ path }) => path.endsWith("/GetInboxIds"))
        .length - before;
    assert.equal(received, (cycle + 1) * width);
    rows.push({ cycle, requests: received, maps: results.length });
    assert.deepEqual(counts(), {
      rustFutureResolvers: 0,
      foreignFutureTasks: 0,
    });
  }
  assert.equal(credentialCalls, auth === "yes" ? 1 : 0);
  assert.equal(peakPending, width);
  assert.ok(heartbeatWhilePending > 0, "JS heartbeat did not run during calls");
  succeeded = true;
} catch (error) {
  primaryFailed = true;
  throw error;
} finally {
  await cleanup([
    async () => {
      if (client) await client.end();
    },
    () => backend.close(),
  ]);
}
for (let attempt = 0; attempt < 100 && counts().foreignFutureTasks; attempt++)
  await delay(10);
assert.deepEqual(counts(), { rustFutureResolvers: 0, foreignFutureTasks: 0 });
console.log(
  JSON.stringify({
    result: succeeded ? "PASS" : "FAIL",
    route,
    auth,
    width,
    cycles,
    rows,
    credentialCalls,
    signerIdentityCalls,
    heartbeats,
    heartbeatWhilePending,
    peakPending,
    counts: counts(),
    nativeCallbackCounts: "UNVERIFIED",
    node: process.version,
  }),
);
