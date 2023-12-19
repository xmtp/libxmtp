import { readFile, writeFile } from "node:fs/promises";

const cargoTomlContent = await readFile("./Cargo.toml", "utf8");
const cargoPackageName = /\[package\]\nname = "(.*?)"/.exec(
  cargoTomlContent
)[1];
const name = cargoPackageName.replace(/-/g, "_");

const content = await readFile(`./dist/node/${name}.js`, "utf8");

const patched = content
  // use global TextDecoder TextEncoder
  .replace(
    "let imports = {};",
    `import { readFile } from "node:fs/promises";
import { dirname, join }  from "node:path";
import { fileURLToPath } from 'url';
let imports = {};
`
  )
  .replace(/const \{ TextDecoder \} = require\(`util`\);\n/, "")
  // attach to `imports` instead of module.exports
  .replace("= module.exports", "= imports")
  .replace(/\nmodule\.exports\.(.*?)\s+/g, "\nexport const $1 = imports.$1 ")
  .replace(/$/, "export default imports;")
  .replace(
    /\nconst path.*\nconst bytes.*\n/,
    `
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const path = join(__dirname, "${name}_bg.wasm");
const bytes = await readFile(path);
`
  );

await writeFile(`./dist/node/${name}.mjs`, patched);
