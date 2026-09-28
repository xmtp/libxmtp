export default {
  resolve: { preserveSymlinks: true },
  test: {
    include: [
      "crates/xmtp_sdk/conformance/browser/bridge.test.ts",
      "target/sdk-generated/typescript-wasm/conformance.gen.test.ts",
    ],
  },
};
