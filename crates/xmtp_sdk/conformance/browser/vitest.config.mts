export default {
  resolve: { preserveSymlinks: true },
  test: {
    include: [
      "crates/xmtp_sdk/conformance/browser/bridge.test.ts",
      "crates/xmtp_sdk/conformance/browser/public-projection.test.ts",
      "crates/xmtp_sdk/conformance/browser/public-objects.test.ts",
      "crates/xmtp_sdk/conformance/browser/codec-policy.test.ts",
      "target/sdk-generated/typescript-wasm/conformance.gen.test.ts",
    ],
  },
};
