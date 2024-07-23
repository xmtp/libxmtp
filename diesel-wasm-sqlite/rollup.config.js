import { defineConfig } from "rollup";
import resolve from "@rollup/plugin-node-resolve";
import { base64 } from "rollup-plugin-base64";

export default defineConfig([
  {
    input: "package.js",
    output: {
      file: "src/package.js",
      format: "es",
    },
    plugins: [
      resolve(),
      base64({ include: "**/*.wasm" }),
    ],
    // external: ["@xmtp/wa-sqlite", "@xmtp/wa-sqlite/build"],
  },
]);
