import { defineConfig } from "rollup";
import { nodeResolve } from "@rollup/plugin-node-resolve";

export default defineConfig([
  {
    input: "package.js",
    output: {
      file: "src/wa-sqlite-diesel-bundle.js",
      format: "es",
    },
    plugins: [
      nodeResolve(),
    ],
    // external: ["@sqlite.org/sqlite-wasm"],
  },
  {
    input:
      "./node_modules/@sqlite.org/sqlite-wasm/sqlite-wasm/jswasm/sqlite3-opfs-async-proxy.js",
    output: {
      file: "src/sqlite3-opfs-async-proxy.js",
      format: "es",
    },
  },
]);
