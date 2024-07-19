const esbuild = require("esbuild");

esbuild.build({
  entryPoints: ["package.js"],
  bundle: true,
  outfile: "src/package.js",
  format: "esm",
  minify: true,
}).catch(() => process.exit(1));
