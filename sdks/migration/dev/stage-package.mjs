#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
  chmodSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const target = process.argv[2];
if (!["node", "browser"].includes(target))
  throw new Error("expected node or browser");
const output = join(root, "target/migration-packages", target);
const generated = join(root, "target/migration-generated", target);
const runtimes = target === "node" ? ["core", "node"] : ["core", "wasm"];
const files = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((item) =>
    item.isDirectory() ? files(join(dir, item.name)) : [join(dir, item.name)],
  );
function writable(dir) {
  chmodSync(dir, 0o755);
  for (const item of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, item.name);
    if (item.isDirectory()) writable(path);
    else chmodSync(path, 0o644);
  }
}
rmSync(output, { recursive: true, force: true });
mkdirSync(output, { recursive: true });
execFileSync(
  "nix",
  ["build", "--no-link", ...runtimes.map((name) => `.#ubjs-${name}`)],
  { cwd: root, stdio: "inherit" },
);
for (const name of runtimes) {
  const store = execFileSync("nix", ["path-info", `.#ubjs-${name}`], {
    cwd: root,
    encoding: "utf8",
  }).trim();
  const from = join(store, `lib/node_modules/@ubjs/${name}`);
  const to = join(output, "node_modules/@ubjs", name);
  rmSync(to, { recursive: true, force: true });
  cpSync(from, to, {
    recursive: true,
    dereference: true,
    filter: (path) =>
      !relative(from, path)
        .split(/[\\/]/)
        .some((part) => ["tests", "node_modules"].includes(part)),
  });
  writable(to);
}
const compile = mkdtempSync(join(root, "target/.migration-compile-"));
try {
  cpSync(join(root, "sdks/migration", target, "src"), compile, {
    recursive: true,
  });
  cpSync(generated, join(compile, "generated"), { recursive: true });
  symlinkSync(join(output, "node_modules"), join(compile, "node_modules"));
  const tsconfig = join(compile, "tsconfig.json");
  writeFileSync(
    tsconfig,
    JSON.stringify({
      compilerOptions: {
        target: "ES2022",
        module: "ESNext",
        moduleResolution: "Bundler",
        strict: true,
        skipLibCheck: true,
        lib: ["ES2022", "DOM", "DOM.Iterable"],
        types: [],
      },
    }),
  );
  writeFileSync(join(compile, "package.json"), '{"type":"module"}');
  const config = join(compile, "tsdown.mjs");
  const entry = files(compile).filter(
    (path) => path.endsWith(".ts") && !path.endsWith(".d.ts"),
  );
  writeFileSync(
    config,
    `export default { ...${JSON.stringify({ entry, unbundle: true, root: compile, cwd: compile, fixedExtension: false, format: "esm", platform: target, outDir: output, dts: true, tsconfig, clean: false, deps: { neverBundle: ["@ubjs/core", "@ubjs/node", "@ubjs/wasm", "@ubjs/wasm/core", "@ubjs/wasm/browser"], onlyBundle: false } })}, inputOptions: { external: (id) => id.endsWith("xmtp_legacy_migration_bg.js") || id.includes("/snippets/") || id.startsWith("@ubjs/") } };`,
  );
  execFileSync(
    process.execPath,
    [join(root, "node_modules/tsdown/dist/run.mjs"), "--config", config],
    { cwd: compile, stdio: "inherit" },
  );
  for (const path of files(generated).filter((path) => !path.endsWith(".ts"))) {
    const to = join(output, "generated", relative(generated, path));
    mkdirSync(dirname(to), { recursive: true });
    cpSync(path, to);
  }
  if (target === "node") {
    const name =
      process.platform === "darwin"
        ? "libxmtp_legacy_migration.dylib"
        : process.platform === "win32"
          ? "xmtp_legacy_migration.dll"
          : "libxmtp_legacy_migration.so";
    cpSync(join(root, "target/debug", name), join(output, "generated", name));
  }
  const manifest = JSON.parse(
    readFileSync(join(root, "sdks/migration", target, "package.json")),
  );
  manifest.main = "./index.js";
  manifest.types = "./index.d.ts";
  manifest.exports = { ".": { types: "./index.d.ts", import: "./index.js" } };
  delete manifest.scripts;
  manifest.dependencies = Object.fromEntries(
    runtimes.map((name) => [
      `@ubjs/${name}`,
      JSON.parse(
        readFileSync(join(output, "node_modules/@ubjs", name, "package.json")),
      ).version,
    ]),
  );
  manifest.bundledDependencies = runtimes.map((name) => `@ubjs/${name}`);
  writeFileSync(
    join(output, "package.json"),
    JSON.stringify(manifest, null, 2),
  );
  const dist = join(root, "sdks/migration", target, "dist");
  rmSync(dist, { recursive: true, force: true });
  symlinkSync(relative(dirname(dist), output), dist, "dir");
  console.log(`Migration package: ${output}`);
} finally {
  rmSync(compile, { recursive: true, force: true });
}
