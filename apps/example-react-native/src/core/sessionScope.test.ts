import assert from "node:assert/strict";
import { test } from "node:test";

import { SessionScope } from "./sessionScope.ts";

test("completed tasks are not retained", async () => {
  const scope = new SessionScope();
  await scope.add(() => Promise.resolve());
  assert.equal(scope.taskCount, 0);
});

test("teardown aborts tasks, runs cleanups, awaits tasks, then final cleanups", async () => {
  const scope = new SessionScope();
  const order: string[] = [];
  void scope.add(
    (signal) =>
      new Promise<void>((resolve) => {
        signal.addEventListener("abort", () => {
          // The task finishes some time after the abort.
          setTimeout(() => {
            order.push("task ended");
            resolve();
          }, 0);
        });
      }),
  );
  scope.addFinalCleanup(() => {
    order.push("final");
  });
  scope.addCleanup(async () => {
    await Promise.resolve();
    order.push("cleanup 1");
  });
  scope.addCleanup(() => {
    order.push("cleanup 2");
  });
  await scope.cancelAndWait();
  assert.deepEqual(order, ["cleanup 1", "cleanup 2", "task ended", "final"]);
  assert.ok(scope.signal.aborted);
});

test("a failing cleanup or task does not stop teardown", async () => {
  const scope = new SessionScope();
  const order: string[] = [];
  const warn = console.warn;
  console.warn = () => undefined;
  try {
    void scope.add(() => Promise.reject(new Error("task failed")));
    scope.addCleanup(() => {
      throw new Error("cleanup failed");
    });
    scope.addCleanup(() => {
      order.push("second cleanup");
    });
    scope.addFinalCleanup(() => {
      order.push("final");
    });
    await scope.cancelAndWait();
  } finally {
    console.warn = warn;
  }
  assert.deepEqual(order, ["second cleanup", "final"]);
});

test("work added after close starts aborted and cleanups run at once", async () => {
  const scope = new SessionScope();
  await scope.cancelAndWait();
  let aborted = false;
  await scope.add((signal) => {
    aborted = signal.aborted;
    return Promise.resolve();
  });
  assert.ok(aborted);
  let cleaned = false;
  scope.addCleanup(() => {
    cleaned = true;
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.ok(cleaned);
  assert.ok(scope.closed);
});
