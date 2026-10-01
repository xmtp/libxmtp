import assert from "node:assert/strict";
import { setImmediate as nextTurn } from "node:timers/promises";

import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";

export const lifetimeCycles = 20;
export const lifetimeCalls = 32;
export const lifetimeDeadline = 30_000;

export function signal() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

export async function within<T>(
  promise: Promise<T>,
  label: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} timed out`)),
          lifetimeDeadline,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export function rawOptions(
  backendURL: string,
  storage: B.StorageOptions = B.StorageOptions.new({
    location: B.StorageLocation.InMemory.new(),
  }),
): B.ClientOptions {
  return {
    ...B.ClientOptions.new({
      backend: B.BackendSource.Options.new({
        options: B.BackendOptions.new({ url: backendURL }),
      }),
      storage,
      deviceSync: false,
    }),
  };
}

export async function foreignTasksDrained() {
  await within(
    (async () => {
      while (B.sdkConformanceForeignCallCounts().inFlight !== 0n)
        await nextTurn();
    })(),
    "foreign callback return",
  );
}

export async function drained() {
  await within(
    (async () => {
      for (;;) {
        const tasks = B.sdkConformanceForeignCallCounts();
        const handles = B.sdkConformanceCallbackHandleCounts();
        if (
          tasks.inFlight === 0n &&
          tasks.running === 0n &&
          Object.values(handles).every((count) => count === 0)
        ) {
          assert.equal(
            tasks.droppedEarly,
            0n,
            "a foreign future dropped before completion",
          );
          assert.equal(
            tasks.pollsOnCallerThread,
            0n,
            "a foreign future polled on the caller thread",
          );
          return;
        }
        await nextTurn();
      }
    })(),
    "foreign tasks and callback handles drain",
  ).catch((error: unknown) => {
    console.error(
      JSON.stringify(
        {
          tasks: B.sdkConformanceForeignCallCounts(),
          handles: B.sdkConformanceCallbackHandleCounts(),
        },
        (_key, value: unknown) =>
          typeof value === "bigint" ? value.toString() : value,
      ),
    );
    throw error;
  });
}

export function heldCounts(minimum: number) {
  const tasks = B.sdkConformanceForeignCallCounts();
  assert.ok(tasks.inFlight >= BigInt(minimum));
  assert.ok(tasks.running >= BigInt(minimum));
  assert.ok(B.sdkConformanceCallbackHandleCounts().foreignFutures >= minimum);
}

export function dispose(value: object) {
  const destroy: unknown = Reflect.get(value, "uniffiDestroy");
  assert.equal(
    typeof destroy,
    "function",
    "native probe expected an owned raw object",
  );
  Reflect.apply(destroy as () => void, value, []);
}
