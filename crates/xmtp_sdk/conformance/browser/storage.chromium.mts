import assert from "node:assert/strict";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";

const browser = await chromium.launch({ headless: true });
try {
  const context = await browser.newContext();
  await context.route("http://127.0.0.1:39999/**", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "text/html",
      body: "<!doctype html><title>Bridge storage test</title>",
    });
  });
  const first = await context.newPage();
  const second = await context.newPage();
  await Promise.all([
    first.goto("http://127.0.0.1:39999/"),
    second.goto("http://127.0.0.1:39999/"),
  ]);
  const held = first.evaluate(async () => {
    const directory = await navigator.storage.getDirectory();
    await directory.getFileHandle("bridge-storage-test", { create: true });
    await navigator.locks.request("xmtp:bridge-storage-test", async () => {
      await new Promise<void>((resolve) => {
        Reflect.set(globalThis, "bridgeReleaseLock", resolve);
      });
    });
  });
  await first.waitForFunction(
    () => typeof Reflect.get(globalThis, "bridgeReleaseLock") === "function",
  );
  const secondResult = await second.evaluate(async () => {
    await navigator.storage.getDirectory();
    return navigator.locks.request(
      "xmtp:bridge-storage-test",
      { ifAvailable: true },
      (lock) => (lock ? "acquired" : "storageBusy"),
    );
  });
  assert.equal(secondResult, "storageBusy");
  await first.evaluate(() => {
    const release: unknown = Reflect.get(globalThis, "bridgeReleaseLock");
    if (typeof release === "function") release();
  });
  await held;
  console.log("Chromium OPFS and second-tab Web Lock check passed");
} finally {
  await browser.close();
}
