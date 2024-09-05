import { defineConfig } from "rollup";
import { nodeResolve } from "@rollup/plugin-node-resolve";
import copy from "rollup-plugin-copy";

export default defineConfig([
  {
    input: "package.js",
    output: {
      file: "src/js/wa-sqlite-diesel-bundle.js",
      format: "es",
    },
    plugins: [
      nodeResolve(),
      copy({
        targets: [
          {
            src:
              "./node_modules/@sqlite.org/sqlite-wasm/sqlite-wasm/jswasm/sqlite3.wasm",
            dest: "src/js",
          },
        ],
      }),
    ],
    // external: ["@sqlite.org/sqlite-wasm"],
  },
  {
    input:
      "./node_modules/@sqlite.org/sqlite-wasm/sqlite-wasm/jswasm/sqlite3-opfs-async-proxy.js",
    output: {
      file: "src/js/sqlite3-opfs-async-proxy.js",
      format: "es",
    },
  },
]);
