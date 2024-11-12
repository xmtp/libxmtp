import { readdirSync, existsSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const distDir = join(__dirname, "..", "dist");

const files = readdirSync(distDir, { recursive: true });

let targetDir = "";
files.some((file) => {
  if (file.endsWith("sqlite3-diesel.js")) {
    targetDir = join(distDir, dirname(file));
    return true;
  }
});

if (!targetDir) {
  console.error("Unable to find target directory for `sqlite3.wasm`");
  process.exit(1);
}

const nodeModules = join(__dirname, "..", "node_modules");
const sourceFile = join(
  nodeModules,
  "@sqlite.org",
  "sqlite-wasm",
  "sqlite-wasm",
  "jswasm",
  "sqlite3.wasm"
);

if (!existsSync(sourceFile)) {
  console.error(`Unable to find "sqlite3.wasm" in "${dirname(sourceFile)}"`);
  process.exit(1);
}

console.log(`Copying "sqlite3.wasm" to "${targetDir}"`);
copyFileSync(sourceFile, join(targetDir, "sqlite3.wasm"));
