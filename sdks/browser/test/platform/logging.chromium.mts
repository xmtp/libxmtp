import assert from "node:assert/strict";

// @ts-expect-error Playwright's explicit ESM file has no declaration file.
import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";

async function bounded<T>(operation: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Chromium logging proof exceeded 20 seconds")),
          20_000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/sdk-browser-logging-vite",
  resolve: {
    alias: {
      "@ubjs/core": `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/core`,
    },
  },
  optimizeDeps: { noDiscovery: true },
  server: { host: "127.0.0.1", port: 0, fs: { strict: false } },
});
await server.listen();
const browser = await chromium.launch({ headless: true });
try {
  const address = server.httpServer?.address();
  assert.ok(address && typeof address !== "string");
  const page = await browser.newPage();
  page.on("pageerror", (error: Error) =>
    console.error("Chromium page:", error),
  );
  page.on("console", (message: { type(): string; text(): string }) => {
    if (
      message.type() === "error" &&
      message.text().startsWith("Logging worker")
    )
      console.error("Chromium console:", message.text());
  });
  await page.addInitScript(() => {
    const OriginalWorker = globalThis.Worker;
    globalThis.Worker = class extends OriginalWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        this.addEventListener("message", (event) => {
          if (event.data?.t === "fatal" || event.data?.t === "refused")
            console.error(
              "Logging worker startup:",
              JSON.stringify(event.data),
            );
        });
        this.addEventListener("error", (event) =>
          console.error("Logging worker:", event.message, event.filename),
        );
      }
    };
  });
  await page.goto(
    `http://127.0.0.1:${address.port}/sdks/browser/test/platform/bridge.chromium.html`,
  );
  await bounded(
    page.evaluate(async () => {
      const sdk =
        await import("../../../../target/sdk-generated/typescript-wasm/index");
      const owner = await sdk.Storage.admin();
      try {
        await sdk.initLogging({ level: "error" });
        await (await import("./public-entry.chromium.ts")).loggingSecrets();
      } finally {
        await owner.end();
      }
    }),
  );
  console.log(
    "Chromium public app logs retain both secret checks with bounded handoffs",
  );
  await bounded(
    page.evaluate(async () =>
      (await import("./logging.chromium.ts")).managedQueue(),
    ),
  );
  console.log(
    "Chromium public setter, real Rust backlog, independent end, and managed retirement passed",
  );
  await bounded(
    page.evaluate(async () =>
      (await import("./logging.chromium.ts")).managedRestart(),
    ),
  );
  console.log(
    "Chromium accepted logging settings survive retirement; clear stays cleared",
  );
  await bounded(
    page.evaluate(async () =>
      (await import("./logging.chromium.ts")).managedFailureRestart(),
    ),
  );
  console.log(
    "Chromium failed-worker restart preserves one app callback and independent creation",
  );
} finally {
  await browser.close();
  await server.close();
}
