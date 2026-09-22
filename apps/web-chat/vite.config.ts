import path from "node:path";
import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig, mergeConfig } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";
import { defineConfig as defineVitestConfig } from "vitest/config";

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  define: {
    "import.meta.env.XMTP_BACKEND_URL": JSON.stringify(
      process.env.XMTP_BACKEND_URL ?? "",
    ),
  },
  plugins: [tsconfigPaths(), react()],
  optimizeDeps: {
    exclude: ["@xmtp/wasm-bindings"],
  },
  server: {
    allowedHosts: true,
    fs: {
      allow: [
        path.resolve(fileURLToPath(new URL(".", import.meta.url)), "../.."),
      ],
    },
  },
  build: {
    sourcemap: true,
  },
});

const vitestConfig = defineVitestConfig({
  test: {
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
