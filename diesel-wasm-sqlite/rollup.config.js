import { defineConfig } from "rollup";
import { nodeResolve } from "@rollup/plugin-node-resolve";
import { base64 } from "rollup-plugin-base64";

export default defineConfig([
  {
    input: "package.js",
    output: {
      file: "src/wa-sqlite-diesel-bundle.js",
      format: "es",
    },
    plugins: [
      nodeResolve(),
      base64({ include: "**/*.wasm" }),
    ],
    // external: ["@xmtp/wa-sqlite", "@xmtp/wa-sqlite/build"],
  },
]);
