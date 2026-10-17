import { fileURLToPath } from "node:url";

import { defineConfig } from "vitest/config";

// The Node host has the same generated native public projection. Only its
// factory/end boundaries are replaced in these React render tests. Hermes
// consumers separately prove that the installed RN entry loads on a device.
export default defineConfig({
  resolve: {
    alias: {
      "@xmtp/react-native-sdk":
        process.env.XMTP_REACT_NATIVE_TEST_ROOT ??
        fileURLToPath(new URL("../../../node/dist/entry.js", import.meta.url)),
    },
  },
  test: { include: ["test/react/**/*.test.tsx"] },
});
