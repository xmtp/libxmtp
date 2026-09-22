import { defineConfig, type UserConfig } from "tsdown";

// Separate builds keep import.meta.url beside index.js for worker URLs.
export default defineConfig(
  ["index", "workers/client", "workers/opfs"].map((entry): UserConfig => ({
    entry: { [entry]: `src/${entry}.ts` },
    platform: "browser",
    target: "esnext",
    format: "esm",
    fixedExtension: false,
    sourcemap: true,
    minify: true,
    dts: entry === "index" ? { generator: "tsgo", sourcemap: false } : false,
    deps: { neverBundle: ["@xmtp/wasm-bindings"] },
  })),
);
