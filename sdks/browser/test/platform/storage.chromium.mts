import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/sdk-browser-storage-vite",
  resolve: {
    alias: {
      "@ubjs/core": `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/core`,
    },
  },
  optimizeDeps: { noDiscovery: true },
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
const url = `http://127.0.0.1:${address.port}/sdks/browser/test/platform/bridge.chromium.html`;
const base = `bridge-${crypto.randomUUID()}`;
const busyFields = {
  code: "StorageBusy",
  category: 2,
  retryable: true,
  typed: true,
};
const opfsAttempt = (page: typeof first, path: string) =>
  page.evaluate(async (databasePath) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    try {
      await bridge.open(databasePath);
      return { code: "opened", category: -1, retryable: false, typed: false };
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
        typed: bridge.isStorageBusy(error),
      };
    }
  }, path);

async function failedPoolAttempt(page: typeof first, path: string) {
  await page.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).holdFailureTermination(),
  );
  const attempt = opfsAttempt(page, path);
  try {
    await page.evaluate(async () =>
      (
        await import("./storage.bridge.chromium.ts")
      ).waitForFailureTermination(),
    );
    const lockName = await first.evaluate(async () => {
      const locks = await navigator.locks.query();
      return locks.held?.find((lock) => lock.name?.startsWith("xmtp:"))?.name;
    });
    assert.ok(
      lockName,
      "failed worker released its Web Lock before termination",
    );
    assert.equal(
      await first.evaluate(
        async (name) =>
          navigator.locks.request(
            name,
            { ifAvailable: true },
            (lock) => !!lock,
          ),
        lockName,
      ),
      false,
      "another owner acquired the failed worker's pool",
    );
  } finally {
    await page.evaluate(async () =>
      (
        await import("./storage.bridge.chromium.ts")
      ).releaseFailureTermination(),
    );
  }
  return attempt;
}

const generation = (page: typeof first) =>
  page.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).workerGenerations(),
  );
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
  // verifies: STORE-007
  await first.evaluate(async (path) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    const before = await bridge.poolFilenames();
    await bridge.rejectBuildWithoutStoredIdentity(path);
    const after = await bridge.poolFilenames();
    if (JSON.stringify(after) !== JSON.stringify(before))
      throw new Error(
        "build created an OPFS database without a stored identity",
      );
  }, `${base}-missing.db`);
  console.log("Chromium missing-identity build left the OPFS pool unchanged");
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
    "StorageBusy",
    "another path in a second tab must be busy",
  );
  assert.deepEqual(await opfsAttempt(second, pathB), busyFields);
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  assert.equal(
    await attempt(),
    "StorageBusy",
    "another client still owns the pool",
  );
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  let reopened: unknown;
  for (let index = 0; index < 50; index++) {
    reopened = await attempt();
    if (reopened === "opened") break;
    if (reopened !== "StorageBusy") break;
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
    "StorageBusy",
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
    await failedPoolAttempt(second, unpausePath),
    busyFields,
    "unpause must report a busy OPFS pool",
  );
  await second.evaluate(async (path) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    await bridge.rejectBuildWithoutStoredIdentity(path, "StorageBusy");
  }, `${base}-unpause-build.db`);
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).release(),
  );
  const failedResumeGeneration = await generation(second);
  assert.deepEqual(await opfsAttempt(second, unpausePath), {
    code: "opened",
    category: -1,
    retryable: false,
    typed: false,
  });
  assert.equal(await generation(second), failedResumeGeneration + 1);
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );

  await third.goto(url);
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).hold(),
  );
  const installPath = `${base}-install.db`;
  assert.deepEqual(
    await failedPoolAttempt(third, installPath),
    busyFields,
    "a fresh WASM worker must report a busy OPFS pool",
  );
  await first.evaluate(async () =>
    (await import("./storage.opfs.hog.chromium.ts")).release(),
  );
  const failedInstallGeneration = await generation(third);
  assert.deepEqual(await opfsAttempt(third, installPath), {
    code: "opened",
    category: -1,
    retryable: false,
    typed: false,
  });
  assert.equal(await generation(third), failedInstallGeneration + 1);
  console.log("Chromium replaced workers after failed OPFS install and resume");

  // A failed registration must end the client it built before the Web Lock
  // is released. Otherwise its SQLite connections keep the OPFS pool.
  await Promise.all(
    [second, third].map((page) =>
      page.evaluate(async () =>
        (await import("./storage.bridge.chromium.ts")).stop(),
      ),
    ),
  );
  assert.equal(
    await first.evaluate(
      async (path) =>
        (await import("./storage.bridge.chromium.ts")).failRegistration(path),
      `${base}-registration.db`,
    ),
    "SignerFailed",
  );
  let afterRegistration: unknown;
  for (let index = 0; index < 50; index++) {
    afterRegistration = await second.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      try {
        await bridge.open(path);
        return "opened";
      } catch (error) {
        return bridge.codeOf(error);
      }
    }, `${base}-after-registration.db`);
    if (afterRegistration !== "StorageBusy") break;
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(
    afterRegistration,
    "opened",
    "a failed registration left its client holding the OPFS pool",
  );
  console.log("Chromium failed registration released the OPFS pool");

  // A build without a stored identity unpauses the pool to look for the
  // file. It must pause the pool again before the Web Lock is released.
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  await first.evaluate(async (path) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    await bridge.rejectBuildWithoutStoredIdentity(path);
  }, `${base}-missing-release.db`);
  let afterMissing: unknown;
  for (let index = 0; index < 50; index++) {
    afterMissing = await second.evaluate(async (path) => {
      const bridge = await import("./storage.bridge.chromium.ts");
      try {
        await bridge.open(path);
        return "opened";
      } catch (error) {
        return bridge.codeOf(error);
      }
    }, `${base}-after-missing.db`);
    if (afterMissing !== "StorageBusy") break;
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(
    afterMissing,
    "opened",
    "a build without a stored identity left the OPFS pool unpaused",
  );
  console.log("Chromium missing-identity build released the OPFS pool");

  // A cancelled create drops its Rust future without the cleanup of a failed
  // create. Its store is open while the signer is pending, so the worker must
  // keep the Web Lock until it ends. The second run blocks the main thread of
  // the page when the create error arrives, as a slow CI runner can. The
  // worker then ends its failure work before the page reads the lock.
  await second.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endOne(),
  );
  for (const [name, slowMainThreadMs] of [
    ["aborted", 0],
    ["aborted-slow", 250],
  ] as const) {
    let aborted: unknown;
    for (let index = 0; index < 50; index++) {
      aborted = await first.evaluate(
        async ({ path, slow }) =>
          (
            await import("./storage.bridge.chromium.ts")
          ).abortCreateWhileSigning(path, slow),
        { path: `${base}-${name}.db`, slow: slowMainThreadMs },
      );
      if (aborted !== "StorageBusy") break;
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
    assert.equal(
      aborted,
      "ended",
      `a cancelled create released the Web Lock while its worker still ran (main thread blocked ${slowMainThreadMs} ms)`,
    );
    let afterAbort: unknown;
    for (let index = 0; index < 50; index++) {
      afterAbort = await second.evaluate(async (path) => {
        const bridge = await import("./storage.bridge.chromium.ts");
        try {
          await bridge.open(path);
          return "opened";
        } catch (error) {
          return bridge.codeOf(error);
        }
      }, `${base}-after-${name}.db`);
      if (afterAbort !== "StorageBusy") break;
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
    assert.equal(
      afterAbort,
      "opened",
      "the ended worker of a cancelled create kept the OPFS pool",
    );
    await second.evaluate(async () =>
      (await import("./storage.bridge.chromium.ts")).endOne(),
    );
  }
  console.log(
    "Chromium cancelled create kept the Web Lock until its worker ended",
  );
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).openAdmins(),
  );
  assert.equal(
    await first.evaluate(async () => {
      const locks = await navigator.locks.query();
      return locks.held?.filter((lock) => lock.name?.startsWith("xmtp:"))
        .length;
    }),
    1,
    "admin creation must hold the shared Web Lock",
  );
  assert.deepEqual(
    await opfsAttempt(second, `${base}-admin-other.db`),
    busyFields,
  );
  const adminPath = `${base}-admin.db`;
  await first.evaluate(async (path) => {
    const bridge = await import("./storage.bridge.chromium.ts");
    await bridge.open(path);
    if (!(await bridge.adminFiles()).includes(path))
      throw new Error("admin file missing");
    await bridge.adminOpenFileIsBusy(path);
    await bridge.endOne();
    await bridge.adminRoundTrip(path);
    await bridge.endAdmin();
    if (!(await bridge.adminFiles()).includes(path))
      throw new Error("second admin closed early");
  }, adminPath);
  assert.deepEqual(
    await opfsAttempt(second, `${base}-admin-other.db`),
    busyFields,
  );
  await first.evaluate(async () =>
    (await import("./storage.bridge.chromium.ts")).endAdmin(),
  );
  assert.equal(
    (await opfsAttempt(second, `${base}-admin-other.db`)).code,
    "opened",
  );
  console.log(
    "Chromium independent admins held the shared pool and guarded open databases",
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
