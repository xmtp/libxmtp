import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/sdk-browser-package-vite",
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
if (!address || typeof address === "string") throw new Error("no Vite port");
const browser = await chromium.launch({
  headless: true,
  args: ["--js-flags=--expose-gc"],
});
const context = await browser.newContext();
const page = await context.newPage();
const nativeLogs: string[] = [];
page.on("console", (message) => nativeLogs.push(message.text()));
const other = await context.newPage();
const url = `http://127.0.0.1:${address.port}/sdks/browser/test/platform/bridge.chromium.html`;
const counts = () =>
  page.evaluate(async () => (await import("./package.chromium.ts")).counts());
/** Waits until every package worker has terminated, collecting garbage first
 * when a dropped object still holds a worker lease. */
async function waitForIdle(gc = false) {
  if (gc) {
    for (let index = 0; index < 100; index++) {
      const idle = await page.evaluate(async () => {
        const collect: unknown = Reflect.get(globalThis, "gc");
        if (typeof collect === "function") collect();
        const { created, terminated } = (
          await import("./package.chromium.ts")
        ).counts();
        return created === terminated;
      });
      if (idle) return;
      await new Promise<void>((resolve) => setTimeout(resolve, 20));
    }
  }
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).waitForIdle(),
  );
}
async function waitForTermination(count: number, gc = false) {
  if (gc) {
    for (let index = 0; index < 100; index++) {
      const actual = await page.evaluate(async () => {
        const collect: unknown = Reflect.get(globalThis, "gc");
        if (typeof collect === "function") collect();
        return (await import("./package.chromium.ts")).counts().terminated;
      });
      if (actual === count) return;
      await new Promise<void>((resolve) => setTimeout(resolve, 20));
    }
  }
  await page.evaluate(
    async (count) =>
      (await import("./package.chromium.ts")).waitForTermination(count),
    count,
  );
}
try {
  await Promise.all([page.goto(url), other.goto(url)]);
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).failFactory(),
  );
  await waitForTermination(1);
  assert.deepEqual(await counts(), { created: 1, terminated: 1 });
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).openAdmins(),
  );
  assert.deepEqual(await counts(), { created: 2, terminated: 1 });
  await page.evaluate(async () => {
    const api = await import("./package.chromium.ts");
    await api.beginDelayedCreate();
    await api.endAdmin();
    await api.endAdmin();
  });
  assert.deepEqual(await counts(), { created: 2, terminated: 1 });
  assert.equal(
    await other.evaluate(async () => {
      const { held } = await navigator.locks.query();
      const name = held?.find((lock) => lock.name?.startsWith("xmtp:"))?.name;
      if (!name) throw new Error("creation reservation lost the Web Lock");
      return navigator.locks.request(
        name,
        { ifAvailable: true },
        (lock) => !!lock,
      );
    }),
    false,
  );
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).finishDelayedCreate(),
  );
  await page.evaluate(
    async (path) => (await import("./package.chromium.ts")).openClient(path),
    `package-${crypto.randomUUID()}.db`,
  );
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).endAdmin(),
  );
  assert.deepEqual(await counts(), { created: 2, terminated: 1 });
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).endClient(),
  );
  await waitForTermination(2);
  // A second real worker must obtain both the Web Lock and OPFS access handles.
  await other.evaluate(
    async (path) => (await import("./package.chromium.ts")).openClient(path),
    `package-second-${crypto.randomUUID()}.db`,
  );
  await other.evaluate(async () =>
    (await import("./package.chromium.ts")).endClient(),
  );
  await other.evaluate(async () =>
    (await import("./package.chromium.ts")).waitForTermination(1),
  );
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).openAdmins(),
  );
  assert.deepEqual(await counts(), { created: 3, terminated: 2 });
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).collectAdmins(),
  );
  await waitForTermination(3, true);
  const publicPath = `package-public-${crypto.randomUUID()}.db`;
  await page.evaluate(
    async (path) => (await import("./package.chromium.ts")).openClient(path),
    publicPath,
  );
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).endClient(),
  );
  await waitForTermination(4);
  await page.evaluate(
    async (path) =>
      (await import("./package.chromium.ts")).publicAdminRoundTrip(path),
    publicPath,
  );
  await waitForTermination(5);
  assert.deepEqual(await counts(), { created: 5, terminated: 5 });
  console.log(
    "Chromium public Storage.admin byte views, guarded methods, independent end, and cleanup passed",
  );
  await page.evaluate(async () =>
    (await import("./package.chromium.ts")).immediateReplacement(),
  );
  console.log(
    "Chromium immediate replacement waited for actual old-worker lock release",
  );
  await page.evaluate(
    async (path) =>
      (await import("./public-projection.chromium.ts")).exercise(path),
    `projection-${crypto.randomUUID()}.db`,
  );
  console.log(
    "Chromium shared projection registered a client through projected signer callbacks",
  );
  // The projection client's worker can terminate after `exercise` returns.
  // Count from an idle state, so a late termination cannot pass a target
  // count (it once made a count wait see 9 workers instead of 8).
  await waitForIdle(true);
  const beforeRoot = await counts();
  await page.evaluate(async () =>
    (await import("./public-root.chromium.ts")).exercise(),
  );
  await waitForIdle();
  assert.deepEqual(await counts(), {
    created: beforeRoot.created + 1,
    terminated: beforeRoot.created + 1,
  });
  console.log(
    "Chromium public root Client ran in the package worker without a session",
  );
  const beforeEntry = await counts();
  const entry = await page.evaluate(async () =>
    (await import("./public-entry.chromium.ts")).exercise(),
  );
  // The connected Backend holds its own worker lease until it is collected.
  await waitForIdle(true);
  assert.equal(
    (await counts()).terminated,
    beforeEntry.terminated + 1,
    "the public entry did not end exactly one package worker",
  );
  assert.equal(
    entry.length,
    11,
    `public entry stopped after: ${entry.join(", ")}`,
  );
  console.log(`Chromium public entry: ${entry.join("; ")}`);
  const failed = await page.evaluate(async () => {
    const { failWorkers } = await import("./package.chromium.ts");
    return (await import("./public-entry.chromium.ts")).failures(failWorkers);
  });
  console.log(
    `Chromium public errors from a failing package worker: ${failed.join(", ")}`,
  );
  const retired = await page.evaluate(async () => {
    const pkg = await import("./package.chromium.ts");
    return (await import("./public-entry.chromium.ts")).retiredWorker(
      () => pkg.counts().terminated,
      pkg.waitForTermination,
    );
  });
  console.log(`Chromium public entry: ${retired.join("; ")}`);
  const restoredValues = await page.evaluate(async () =>
    (await import("./public-entry.chromium.ts")).restored(),
  );
  console.log(`Chromium public entry: ${restoredValues.join("; ")}`);
  await page.evaluate(async () =>
    (await import("./public-entry.chromium.ts")).loggingEnd(),
  );
  // verifies: LOG-010. These logs come from the real SDK operations above.
  for (const target of ["xmtp_sdk::credentials", "xmtp_sdk::signer"])
    assert.ok(
      nativeLogs.some((line) => line.includes(target)),
      target,
    );
  for (const secret of [
    "LOG_CREDENTIAL_SENTINEL_89d42",
    "LOG_SIGNING_KEY_SENTINEL_89d42!!!",
  ]) {
    const bytes = Buffer.from(secret);
    for (const value of [
      secret,
      bytes.toString("hex"),
      `[${[...bytes].join(", ")}]`,
    ])
      assert.ok(
        !nativeLogs.some((line) => line.includes(value)),
        "browser native log exposed a secret",
      );
  }
  console.log(
    "Chromium client end from log callback and native/app secret checks passed",
  );
  console.log(
    "Chromium package reservations, shared owners, final worker termination, replacement, and GC passed",
  );
} finally {
  await browser.close();
  await server.close();
}
