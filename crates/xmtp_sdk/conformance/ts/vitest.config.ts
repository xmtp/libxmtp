import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["crates/xmtp_sdk/conformance/ts/node.test.ts"],
    testTimeout: 120_000,
  },
});
