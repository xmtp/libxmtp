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
      "apps/xmtp_sdk_bindgen/runtime-tests/ts/bridge.test.ts",
      "apps/xmtp_sdk_bindgen/runtime-tests/ts/public-projection.test.ts",
      "apps/xmtp_sdk_bindgen/runtime-tests/ts/public-objects.test.ts",
      "apps/xmtp_sdk_bindgen/runtime-tests/ts/codec-policy.test.ts",
      "apps/xmtp_sdk_bindgen/runtime-tests/ts/reader-consumer.test.ts",
      "target/sdk-generated/typescript-wasm/conformance.gen.test.ts",
    ],
  },
};
