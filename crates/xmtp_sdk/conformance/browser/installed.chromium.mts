import assert from "node:assert/strict";
import { resolve } from "node:path";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

const consumer = process.env.SDK_INSTALLED_CONSUMER;
if (!consumer)
  throw new Error(
    "SDK_INSTALLED_CONSUMER must name the clean installed consumer",
  );
const server = await createServer({
  root: resolve(consumer),
  configFile: false,
  optimizeDeps: { noDiscovery: true },
  server: {
    host: "127.0.0.1",
    port: 0,
    proxy: {
      "/backend": {
        target: process.env.XMTP_BACKEND_URL,
        changeOrigin: true,
        rewrite: (path: string) => path.replace(/^\/backend/, ""),
      },
    },
  },
});
await server.listen();
const address = server.httpServer?.address();
if (!address || typeof address === "string")
  throw new Error("No server address");
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext();
const page = await context.newPage();
const other = await context.newPage();
const failures: string[] = [];
const workers = new Set<string>();
page.on("pageerror", (error) => failures.push(error.message));
page.on("worker", (worker) => workers.add(worker.url()));
const url = `http://127.0.0.1:${address.port}/`;
try {
  await Promise.all([page.goto(url), other.goto(url)]);
  await page.evaluate(async () =>
    (await import(String("/installed-page.ts"))).admin(),
  );
  await page.evaluate(async () =>
    (await import(String("/installed-page.ts"))).open(),
  );
  assert.equal(
    await other.evaluate(async () =>
      (await import(String("/installed-page.ts"))).busy(),
    ),
    true,
  );
  const receipt = await page.evaluate(async () =>
    (await import(String("/installed-page.ts"))).exercise(),
  );
  assert.equal(receipt.standardEncodeCount, 20);
  assert.equal(receipt.fallbackOverride, true);
  assert.equal(receipt.fallbackThrow, true);
  assert.ok(receipt.attachmentBytes > 0);
  assert.equal(receipt.sent, 20);
  assert.equal(receipt.callbackCount, 20);
  assert.ok(workers.size > 0, "The installed package did not create a worker");
  await page.evaluate(async () =>
    (await import(String("/installed-page.ts"))).end(),
  );
  await other.evaluate(async () =>
    (await import(String("/installed-page.ts"))).open(),
  );
  await other.evaluate(async () =>
    (await import(String("/installed-page.ts"))).end(),
  );
  assert.deepEqual(failures, []);
  console.log(
    JSON.stringify({
      installedPackage: true,
      secondTabStorageBusy: true,
      ownerReplacement: true,
      workerUrls: [...workers],
      ...receipt,
    }),
  );
} finally {
  await browser.close();
  await server.close();
}
