import { playwright } from "../../../../sdks/browser/node_modules/@vitest/browser-playwright/dist/index.js";
import { defineConfig } from "../../../../sdks/browser/node_modules/vitest/dist/config.js";

export default defineConfig({
  cacheDir: "target/sdk-callback-lifetime-vitest",
  resolve: {
    alias: {
      "@ubjs/core": `${process.cwd()}/target/sdk-callback-lifetime/node_modules/@ubjs/core`,
    },
  },
  test: {
    include: [
      "crates/xmtp_sdk/conformance/browser/callback-lifetime.browser.test.ts",
    ],
    browser: {
      enabled: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
      headless: true,
    },
  },
});
