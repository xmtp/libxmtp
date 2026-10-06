export default {
  resolve: {
    preserveSymlinks: true,
    // Both runtimes use one player entry under --preserve-symlinks.
    alias: [
      {
        find: /^@ubjs\/core$/,
        replacement: `${process.cwd()}/target/sdk-generated/typescript-wasm/node_modules/@ubjs/core/dist/esm/index.js`,
      },
    ],
  },
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
