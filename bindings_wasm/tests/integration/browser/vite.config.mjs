import { defineConfig, mergeConfig } from "vite";
import { defineConfig as defineVitestConfig } from "vitest/config";

// https://vitejs.dev/config/
const viteConfig = defineConfig({});

const vitestConfig = defineVitestConfig({
  optimizeDeps: {
    exclude: ["@xmtp/wasm-bindings"],
  },
  test: {
    browser: {
      provider: "playwright",
      enabled: true,
      headless: true,
      screenshotFailures: false,
      instances: [
        {
          browser: "chromium",
        },
        {
          browser: "firefox",
        },
      ],
    },
    testTimeout: 120000,
  },
});

export default mergeConfig(viteConfig, vitestConfig);
