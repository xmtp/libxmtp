import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: { tsconfigPaths: true },
  test: {
    globals: true,
    testTimeout: 60000,
    hookTimeout: 30000,
    globalSetup: ["./vitest.setup.ts"],
  },
});
