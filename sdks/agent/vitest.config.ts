import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    tsconfigPaths: true,
  },
  test: {
    globals: true,
    environment: "node",
    testTimeout: 120000,
    hookTimeout: 60000,
    globalSetup: ["./vitest.setup.ts"],
  },
});
