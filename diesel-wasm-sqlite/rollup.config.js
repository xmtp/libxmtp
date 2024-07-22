import { defineConfig } from "rollup";
import resolve from "@rollup/plugin-node-resolve";
import copy from "rollup-plugin-copy";

export default defineConfig([
  {
    input: "package.js",
    output: {
      file: "src/package.js",
      format: "es",
    },
    plugins: [
      resolve(),
      copy({
        targets: [
          {
            src: "node_modules/@xmtp/wa-sqlite/dist/wa-sqlite.wasm",
            dest: "src",
          },
        ],
      }),
    ],
    // external: ["@xmtp/wa-sqlite", "@xmtp/wa-sqlite/build"],
  },
]);
