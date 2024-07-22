const esbuild = require("esbuild");
const { wasmLoader } = require("esbuild-plugin-wasm");

esbuild.build({
  entryPoints: ["package.js"],
  bundle: true,
  outfile: "src/package.js",
  plugins: [
    wasmLoader(),
  ],
  format: "esm",
  minify: true,
}).catch(() => process.exit(1));
