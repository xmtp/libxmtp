import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { playwright } from "@vitest/browser-playwright";
import { defineConfig, mergeConfig } from "vite";
import { defineConfig as defineVitestConfig } from "vitest/config";

import { recoveryProxyCommands } from "./test/recovery-proxy";

// Keep package asset URLs inside the workspace during browser tests.
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

// `sdkVersion()` returns the Rust workspace version.
const sdkVersion = /^version = "([^"]+)"$/m.exec(
  readFileSync(new URL("../../Cargo.toml", import.meta.url), "utf8"),
)?.[1];
if (sdkVersion === undefined) throw new Error("workspace version not found");

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  resolve: {
    tsconfigPaths: true,
  },
  define: {
    "import.meta.env.XMTP_BACKEND_URL": JSON.stringify(
      process.env.XMTP_BACKEND_URL,
    ),
    "import.meta.env.XMTP_SDK_VERSION": JSON.stringify(sdkVersion),
  },
  server: {
    fs: {
      allow: [repoRoot],
    },
  },
});

const vitestConfig = defineVitestConfig({
  optimizeDeps: {
    exclude: ["@xmtp/browser-sdk", "@xmtp/browser-sdk/pure"],
  },
  test: {
    include: ["test/*.test.ts"],
    browser: {
      provider: playwright(),
      enabled: true,
      headless: true,
      screenshotFailures: false,
      // Node-side helpers that browser tests call through `vitest/browser`.
      commands: recoveryProxyCommands,
      instances: [
        {
          browser: "chromium",
        },
      ],
    },
    testTimeout: 120000,
  },
});

export default mergeConfig(viteConfig, vitestConfig);
