import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/sdk-browser-smoke-vite",
  resolve: {
    preserveSymlinks: false,
    alias: {
      "@ubjs/core": `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/core`,
    },
  },
  optimizeDeps: { noDiscovery: true },
  server: {
    host: "127.0.0.1",
    port: 0,
    strictPort: false,
    fs: { strict: false },
  },
});
await server.listen();
const address = server.httpServer?.address();
if (!address || typeof address === "string")
  throw new Error("Vite has no port");
const browser = await chromium.launch({ headless: true });
const readerOpeningOnly = process.argv.includes("--reader-opening-only");
try {
  const page = await browser.newPage();
  await page.goto(
    `http://127.0.0.1:${address.port}/crates/xmtp_sdk/conformance/browser/bridge.chromium.html`,
  );
  const backendURL = process.env.XMTP_BACKEND_URL;
  assert.ok(backendURL, "missing worktree backend URL");
  const result = await page.evaluate(async ({ url, readerOpeningOnly }) => {
    const { checkLateReaderOpen, checkClientEndDuringReaderOpen } =
      await import("./stream-opening.chromium.ts");
    // These cases need only the normal worker product, without Rust fixtures.
    if (readerOpeningOnly) {
      await checkLateReaderOpen(url);
      await checkClientEndDuringReaderOpen(url);
      return "reader opening";
    }
    const { checkWorkerFailure } = await import("./bridge.failure.chromium.ts");
    await checkWorkerFailure();
    const { checkPureCodecs } = await import("./pure-codecs.chromium.ts");
    const count = await checkPureCodecs();
    const { checkDeletedMessages } =
      await import("./message.deleted.chromium.ts");
    await checkDeletedMessages(url);
    const { checkMessageStream } = await import("./stream.chromium.ts");
    await checkMessageStream(url);
    await checkLateReaderOpen(url);
    await checkClientEndDuringReaderOpen(url);
    const { checkCustomMessageLift } =
      await import("./message.custom.chromium.ts");
    checkCustomMessageLift();
    const { checkStandardMessageLift, checkStandardMessages } =
      await import("./message.standard.chromium.ts");
    checkStandardMessageLift();
    await checkStandardMessages(url);
    return count;
  }, { url: backendURL, readerOpeningOnly });
  if (readerOpeningOnly) {
    assert.equal(result, "reader opening");
    console.log("Chromium late reader opening and client end during opening passed");
  } else {
    assert.equal(result, 15);
    console.log(
      "Chromium worker failure, 15 pure codecs, deleted messages, message stream, late reader opening, client end during opening, custom lift, and standard messages passed",
    );
  }
} finally {
  await browser.close();
  await server.close();
}
