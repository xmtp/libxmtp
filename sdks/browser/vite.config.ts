import { fileURLToPath } from "node:url";

import { playwright } from "@vitest/browser-playwright";
import { defineConfig, mergeConfig } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";
import { defineConfig as defineVitestConfig } from "vitest/config";

// Workspace-linked bindings live outside this package. Allow the repository
// root so Vite can serve their WASM files.
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  plugins: [tsconfigPaths()],
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
    exclude: ["@xmtp/wasm-bindings"],
  },
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
