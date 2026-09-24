import { fileURLToPath } from "node:url";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig, mergeConfig } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";
import { defineConfig as defineVitestConfig } from "vitest/config";

// Repo root, three levels up from sdks/js/browser-sdk. @xmtp/wasm-bindings is a
// Yarn `portal:` symlink into bindings/wasm, which lives outside the sdks/js
// workspace root; Vite's dev server refuses to serve files outside its fs.allow
// list, so the .wasm fetch fails without this. Allow the repo root.
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

// https://vitejs.dev/config/
const viteConfig = defineConfig({
  plugins: [tsconfigPaths()],
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
