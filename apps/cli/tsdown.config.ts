import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  platform: "node",
  target: "node24",
  format: "esm",
  fixedExtension: false,
  sourcemap: true,
  dts: { generator: "tsgo", sourcemap: true },
  deps: { neverBundle: ["@xmtp/node-bindings", "@xmtp/node-sdk"] },
});
