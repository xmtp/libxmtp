import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

// verifies: P58
const server = await createServer({
  root: process.cwd(),
  configFile: false,
  resolve: { preserveSymlinks: true },
  server: {
    host: "127.0.0.1",
    port: 0,
    fs: { strict: false },
    proxy: {
      "/backend": {
        target: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
        changeOrigin: true,
        rewrite: (path: string) => path.replace(/^\/backend/, ""),
      },
    },
  },
});
await server.listen();
const address = server.httpServer?.address();
if (!address || typeof address === "string")
  throw new Error("Vite has no port");
const browser = await chromium.launch({
  headless: true,
  args: ["--js-flags=--expose-gc"],
});
const context = await browser.newContext();
const first = await context.newPage();
const second = await context.newPage();
const third = await context.newPage();
const url = `http://127.0.0.1:${address.port}/crates/xmtp_sdk/conformance/browser/bridge.chromium.html`;
const base = `bridge-${crypto.randomUUID()}`;
const busyFields = { code: "storageBusy", category: 2, retryable: true };
const opfsAttempt = (page: typeof first, path: string) =>
  page.evaluate(async (databasePath) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    try {
      await bridge.open(databasePath);
      return { code: "opened", category: -1, retryable: false };
    } catch (error) {
      const detail =
        error !== null && typeof error === "object" && "inner" in error
          ? Array.isArray(error.inner)
            ? error.inner[0]
            : error.inner
          : error;
      if (detail === null || typeof detail !== "object")
        throw new Error(`missing error details: ${String(error)}`);
      return {
        code: Reflect.get(detail, "code"),
        category: Reflect.get(detail, "category"),
        retryable: Reflect.get(detail, "retryable"),
      };
    }
  }, path);
try {
  await Promise.all([first.goto(url), second.goto(url)]);
  const pathA = `${base}-a.db`;
  const pathB = `${base}-b.db`;
  const pathC = `${base}-c.db`;
  assert.equal(
    await first.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      return bridge.open(path);
    }, pathA),
    pathA,
  );
  assert.equal(
    await first.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      return bridge.open(path);
    }, pathC),
    pathC,
  );
  const attempt = () =>
    second.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      try {
        await bridge.open(path);
        return "opened";
      } catch (error) {
        return bridge.codeOf(error);
      }
    }, pathB);
  assert.equal(
    await attempt(),
    "storageBusy",
    "another path in a second tab must be busy",
  );
  assert.deepEqual(await opfsAttempt(second, pathB), busyFields);
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  assert.equal(
    await attempt(),
    "storageBusy",
    "another client still owns the pool",
  );
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  let reopened: unknown;
  for (let index = 0; index < 50; index++) {
    reopened = await attempt();
    if (reopened === "opened") break;
    if (reopened !== "storageBusy") break;
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(
    reopened,
    "opened",
    "OPFS install must retry after the lock is released",
  );
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).dropOne(),
  );
  let gcEntered = false;
  for (let index = 0; index < 100; index++) {
    gcEntered = await second.evaluate(async () => {
      const gc: unknown = Reflect.get(globalThis, "gc");
      if (typeof gc !== "function")
        throw new Error("Chromium did not expose gc");
      gc();
      return (await (await import("./storage.bridge.chromium.ts")).gcState())
        .gcEntered;
    });
    if (gcEntered) break;
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
  assert.ok(gcEntered, "Chromium did not collect the Client proxy");
  const pathD = `${base}-d.db`;
  assert.equal(
    await first.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      try {
        await bridge.open(path);
        return "opened";
      } catch (error) {
        return bridge.codeOf(error);
      }
    }, pathD),
    "storageBusy",
    "GC released the lock before Rust closed SQLite",
  );
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).gcAllowClose(),
  );
  let gcReopened: unknown;
  for (let index = 0; index < 100; index++) {
    gcReopened = await first.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      try {
        await bridge.open(path);
        return "opened";
      } catch (error) {
        return bridge.codeOf(error);
      }
    }, pathD);
    if (gcReopened === "opened") break;
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(gcReopened, "opened", "GC close did not release the OPFS pool");
  assert.equal(
    (
      await second.evaluate(async () =>
        (await import("./storage.bridge.chromium.ts")).gcState(),
      )
    ).gcFinished,
    true,
  );
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );

  // The hog does not take a Web Lock. Both failures must come from SQLite SAH.
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).hold(),
  );
  const unpausePath = `${base}-unpause.db`;
  assert.deepEqual(
    await opfsAttempt(second, unpausePath),
    busyFields,
    "unpause must report a busy OPFS pool",
  );
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).release(),
  );
  assert.deepEqual(await opfsAttempt(second, unpausePath), {
    code: "opened",
    category: -1,
    retryable: false,
  });
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );

  await third.goto(url);
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).hold(),
  );
  const installPath = `${base}-install.db`;
  assert.deepEqual(
    await opfsAttempt(third, installPath),
    busyFields,
    "a fresh WASM worker must report a busy OPFS pool",
  );
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).release(),
  );
  assert.deepEqual(await opfsAttempt(third, installPath), {
    code: "opened",
    category: -1,
    retryable: false,
  });
  console.log(
    "Chromium real WASM retried OPFS install and unpause after SAH contention",
  );
} finally {
  await first
    .evaluate(async () =>
      (await import("./storage.opfs.hog.chromium.ts")).stop(),
    )
    .catch(() => {});
  await Promise.all(
    [first, second, third].map((page) =>
      page
        .evaluate(async () => {
          await (await import("./storage.bridge.chromium.ts")).stop();
        })
        .catch(() => {}),
    ),
  );
  await browser.close();
  await server.close();
}
