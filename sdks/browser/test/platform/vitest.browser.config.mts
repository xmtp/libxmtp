import { playwright } from "../../../../sdks/browser/node_modules/@vitest/browser-playwright/dist/index.js";
import { defineConfig } from "../../../../sdks/browser/node_modules/vitest/dist/config.js";

// Vite resolves the Vitest browser client from its root, so the root is the
// package that installs Vitest. Paths stay relative to the repository root.
export default defineConfig({
  root: `${process.cwd()}/sdks/browser`,
  cacheDir: `${process.cwd()}/target/sdk-browser-vitest`,
  define: {
    __XMTP_BACKEND_URL__: JSON.stringify(process.env.XMTP_BACKEND_URL),
    __SDK_FIXTURE_URL__: JSON.stringify(process.env.SDK_FIXTURE_URL),
  },
  resolve: {
    alias: [
      {
        find: "@ubjs/core",
        replacement: `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/core`,
      },
      {
        find: /^@ubjs\/wasm$/,
        replacement: `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/wasm`,
      },
    ],
  },
  optimizeDeps: { include: ["@ubjs/wasm"] },
  test: {
    include: ["test/platform/suite.browser.test.ts"],
    browser: {
      enabled: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
      headless: true,
    },
  },
});
