import { fileURLToPath } from "node:url";

import { playwright } from "@vitest/browser-playwright";
import { defineConfig, mergeConfig } from "vite";
import { defineConfig as defineVitestConfig } from "vitest/config";

// Keep package asset URLs inside the workspace during browser tests.
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  resolve: {
    tsconfigPaths: true,
  },
  define: {
    "import.meta.env.XMTP_BACKEND_URL": JSON.stringify(
      process.env.XMTP_BACKEND_URL,
    ),
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
    include: [
      "test/public*.test.ts",
      "test/device-sync.test.ts",
      "test/auth-public.test.ts",
      "test/content-public.test.ts",
    ],
    browser: {
      provider: playwright(),
      enabled: true,
      headless: true,
      screenshotFailures: false,
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
