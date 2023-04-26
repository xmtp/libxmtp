import typescript from "@rollup/plugin-typescript";
import { wasm } from "@rollup/plugin-wasm";
import path from "path";
import fs from "fs";

const outdir = (fmt, env) => {
  if (env == "node") {
    return `node`;
  } else {
    return `${fmt}${env == "slim" ? "-slim" : ""}`;
  }
};

const rolls = (fmt, env) => ({
  input: env !== "slim" ? "src/index.ts" : "src/index_slim.ts",
  output: {
    dir: `dist`,
    format: fmt,
    entryFileNames:
      outdir(fmt, env) + `/[name].` + (fmt === "cjs" ? "cjs" : "js"),
    name: "bindings_wasm",
  },
  plugins: [
    // We want to inline our wasm bundle as base64. Not needing browser users
    // to fetch an additional asset is a boon as there's less room for errors
    env != "slim" &&
      wasm(
        env == "node"
          ? {
              maxFileSize: 0,
              targetEnv: "node",
              publicPath: "../",
              fileName: "[name][extname]",
            }
          : { targetEnv: "auto-inline" }
      ),
    typescript({
      target: fmt == "es" ? "ES2022" : "ES2017",
      outDir: `dist/${outdir(fmt, env)}`,
      rootDir: "src",
    }),
    {
      name: "copy-pkg",

      // wasm-bindgen outputs a import.meta.url when using the web target.
      // rollup will either perserve the the statement when outputting an esm,
      // which will cause webpack < 5 to choke or it will output a
      // "require('url')", for other output types, causing more choking. Since
      // we want a downstream developer to either not worry about providing wasm
      // at all, or forcing them to deal with bundling, we resolve the import to
      // an empty string. This will error at runtime.
      resolveImportMeta: () => `""`,
      generateBundle() {
        // copy the typescript definitions that wasm-bindgen creates into the
        // distribution so that downstream users can benefit from documentation
        // on the rust code
        fs.mkdirSync(`./dist/types/pkg`, { recursive: true });
        fs.copyFileSync(
          path.resolve("./src/pkg/bindings_wasm.d.ts"),
          path.resolve(`./dist/types/pkg/bindings_wasm.d.ts`)
        );
      },
    },
  ],
});

export default [
  rolls("umd", "fat"),
  rolls("es", "fat"),
  rolls("cjs", "fat"),
  rolls("cjs", "node"),
  rolls("es", "slim"),
  rolls("cjs", "slim"),
];
