import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  platform: "node",
  target: "node22.12",
  format: "esm",
  fixedExtension: false,
  sourcemap: true,
  dts: {
    generator: "tsgo",
    tsconfig: "tsconfig.dts.json",
    sourcemap: false,
  },
  deps: { neverBundle: ["@xmtp/node-bindings"] },
});
