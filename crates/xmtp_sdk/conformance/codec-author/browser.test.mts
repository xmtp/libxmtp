import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";
import { verifyWirePolicy } from "./wire-policy.test.mjs";

const backend = process.env.XMTP_BACKEND_URL;
if (!backend) throw new Error("XMTP_BACKEND_URL is required");
const sdk = `${process.cwd()}/target/sdk-codec-author/browser/node_modules/xmtp-sdk-browser`;
const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/sdk-codec-author/vite",
  resolve: {
    alias: [
      { find: /^@ubjs\/core$/, replacement: `${sdk}/node_modules/@ubjs/core` },
      { find: /^@ubjs\/wasm$/, replacement: `${sdk}/node_modules/@ubjs/wasm` },
    ],
  },
  optimizeDeps: { noDiscovery: true },
  server: {
    host: "127.0.0.1",
    port: 0,
    fs: { strict: false },
    proxy: {
      "/backend": {
        target: backend,
        changeOrigin: true,
        rewrite: (path: string) => path.replace(/^\/backend/, ""),
      },
    },
  },
});
await server.listen();
let closeBrowser = async (): Promise<void> => {};
try {
  const address = server.httpServer?.address();
  assert.ok(address && typeof address !== "string", "no Vite port");
  const browser = await chromium.launch({ headless: true });
  closeBrowser = () => browser.close();
  const page = await browser.newPage();
  await page.goto(
    `http://127.0.0.1:${address.port}/crates/xmtp_sdk/conformance/browser/bridge.chromium.html`,
  );
  const results = await page.evaluate(async () =>
    (await import("../codec-author/browser.test.ts")).run(),
  );
  assert.equal(results.checks.length, 7);
  verifyWirePolicy(results);
  console.log(
    `Chromium independent codec author: ${results.checks.join("; ")}; wire push flags and no default compression`,
  );
} finally {
  await closeBrowser();
  await server.close();
}
