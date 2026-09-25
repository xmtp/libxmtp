import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

const server = await createServer({
  root: process.cwd(),
  configFile: false,
  resolve: { preserveSymlinks: true },
  server: { host: "127.0.0.1", port: 0, strictPort: false, fs: { strict: false } },
});
await server.listen();
const address = server.httpServer?.address();
if (!address || typeof address === "string") throw new Error("Vite has no port");
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.goto(
    `http://127.0.0.1:${address.port}/crates/xmtp_sdk/conformance/browser/bridge.chromium.html`,
  );
  const result = await page.evaluate(async () => {
    const { checkWorkerFailure } = await import("./bridge.failure.chromium.ts");
    await checkWorkerFailure();
    const { checkPureCodecs } = await import("./pure-codecs.chromium.ts");
    return checkPureCodecs();
  });
  assert.equal(result, 15);
  console.log("Chromium worker failure and 15 pure codec proofs passed");
} finally {
  await browser.close();
  await server.close();
}
