#!/usr/bin/env node
import { execFileSync } from "node:child_process";
// Compile matched generated trees and retain their runtime assets.
import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  symlinkSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  copyFileSync,
  cpSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, relative, delimiter } from "node:path";
import { fileURLToPath } from "node:url";

import { checkGeneratedAssets } from "./check-generated-assets.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const generated = resolve(
  process.env.XMTP_SDK_GENERATED_DIR ?? "target/sdk-generated",
);
const output = resolve(
  process.env.XMTP_SDK_PACKAGES_DIR ?? "target/sdk-packages",
);
const target = process.argv[2];
if (!["node", "browser"].includes(target))
  throw new Error("expected node or browser");
const trees =
  target === "node"
    ? ["typescript-napi"]
    : ["typescript-wasm", "typescript-pure"];
const contracts = trees.map((tree) =>
  JSON.parse(readFileSync(join(generated, tree, "sdk-contract.json"))),
);
if (
  contracts.some(
    (record) =>
      record.contract !== contracts[0].contract ||
      record.generator !== contracts[0].generator,
  )
)
  throw new Error("SDK generated contract mismatch");
const hash = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((item) => {
    if (item.name === "node_modules") return [];
    const path = join(directory, item.name);
    return item.isDirectory() ? files(path) : [path];
  });
}
for (let i = 0; i < trees.length; i++) {
  checkGeneratedAssets(generated, trees[i], contracts[i]);
}
mkdirSync(output, { recursive: true });
const staging = mkdtempSync(join(output, ".sdk-stage-"));
const destination = join(staging, target);
const product = join(output, target);
const previous = join(staging, "previous");
try {
  mkdirSync(destination, { recursive: true });
  const compiler = resolve(
    process.env.XMTP_SDK_TSDOWN_CLI ??
      join(root, "node_modules/tsdown/dist/run.mjs"),
  );
  const runtimes = target === "node" ? ["core", "node"] : ["core", "wasm"];
  // The pinned runtime includes the host .node binary. Copy it into the npm
  // product so an empty consumer never uses a link to the build machine.
  if (!process.env.XMTP_SDK_RUNTIME_DIR)
    execFileSync(
      "nix",
      ["build", "--no-link", ...runtimes.map((name) => `.#ubjs-${name}`)],
      { cwd: root, stdio: "inherit" },
    );
  for (const name of runtimes) {
    const store = process.env.XMTP_SDK_RUNTIME_DIR
      ? undefined
      : execFileSync("nix", ["path-info", `.#ubjs-${name}`], {
          cwd: root,
          encoding: "utf8",
        }).trim();
    const input = store ?? resolve(process.env.XMTP_SDK_RUNTIME_DIR);
    const flat = join(input, name);
    const runtimeSource =
      !store && existsSync(join(flat, "package.json"))
        ? flat
        : join(input, `lib/node_modules/@ubjs/${name}`);
    cpSync(runtimeSource, join(destination, "node_modules/@ubjs", name), {
      recursive: true,
      dereference: true,
    });
    const runtime = join(destination, "node_modules/@ubjs", name);
    for (const path of files(runtime)) chmodSync(path, 0o644);
    rmSync(join(runtime, "dist/cjs"), { recursive: true, force: true });
    const manifestFile = join(runtime, "package.json");
    const manifest = JSON.parse(readFileSync(manifestFile));
    if (manifest.exports?.["."]?.require) delete manifest.exports["."].require;
    if (manifest.module) manifest.main = manifest.module;
    writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + "\n");
  }
  const compile = mkdtempSync(join(output, ".sdk-compile-"));
  try {
    for (const tree of trees)
      cpSync(join(generated, tree), join(compile, tree), {
        recursive: true,
        filter: (path) => !path.split(/[\\/]/).includes("node_modules"),
      });
    symlinkSync(
      join(destination, "node_modules"),
      join(compile, "node_modules"),
    );
    const tsconfig = join(compile, "tsconfig.json");
    writeFileSync(
      tsconfig,
      JSON.stringify({
        compilerOptions: {
          target: "ES2022",
          module: "ESNext",
          moduleResolution: "Bundler",
          skipLibCheck: true,
          strict: true,
          allowImportingTsExtensions: true,
          lib: ["ES2022", "DOM", "DOM.Iterable"],
          types: [],
        },
        include: ["**/*.ts"],
      }),
    );
    for (const tree of trees) {
      const source = join(compile, tree);
      const sourceManifest = join(source, "package.json");
      writeFileSync(
        sourceManifest,
        JSON.stringify({
          ...JSON.parse(readFileSync(sourceManifest)),
          type: "module",
        }),
      );
      const dest = target === "node" ? destination : join(destination, tree);
      mkdirSync(dest, { recursive: true });
      const entries = files(source).filter(
        (path) =>
          path.endsWith(".ts") &&
          !path.endsWith(".test.ts") &&
          !path.endsWith(".d.ts"),
      );
      const config = join(compile, `tsdown-${tree}.mjs`);
      writeFileSync(
        config,
        `export default { ...${JSON.stringify({ entry: entries, unbundle: true, root: source, cwd: source, fixedExtension: false, format: "esm", platform: target === "node" ? "node" : "browser", outDir: dest, dts: true, tsconfig, clean: false, deps: { neverBundle: ["@ubjs/core", "@ubjs/node", "@ubjs/wasm", "@ubjs/wasm/core", "@ubjs/wasm/browser", "#xmtp/binding"], onlyBundle: false } })}, inputOptions: { external: (id) => id.endsWith("xmtp_sdk_bg.js") || id.includes("/snippets/") || id.startsWith("@ubjs/") || id === "#xmtp/binding" || (${JSON.stringify(tree === "typescript-wasm")} && id.includes("typescript-pure")) } };\n`,
      );
      execFileSync(process.execPath, [compiler, "--config", config], {
        cwd: source,
        stdio: "inherit",
      });
      for (const path of files(source)) {
        if (
          (path.endsWith(".ts") && !path.endsWith(".d.ts")) ||
          path.endsWith(".json")
        )
          continue;
        const to = join(dest, relative(source, path));
        mkdirSync(dirname(to), { recursive: true });
        copyFileSync(path, to);
      }
      writeFileSync(
        join(dest, "package.json"),
        JSON.stringify(
          {
            private: true,
            type: "module",
            imports: { "#xmtp/binding": "./xmtp_sdk.js" },
          },
          null,
          2,
        ) + "\n",
      );
    }
  } finally {
    rmSync(compile, { recursive: true, force: true });
  }
  if (
    target === "node" &&
    !files(join(destination, "node_modules/@ubjs/node")).some((path) =>
      path.endsWith(".node"),
    )
  ) {
    throw new Error("SDK Node package has no native runtime binary");
  }
  const entry = target === "node" ? "./index.js" : "./typescript-wasm/index.js";
  // Public imports wait for the contract check. The SDK root stays private.
  writeFileSync(
    join(destination, "entry.js"),
    `import './sdk-contract-check.js';\nexport * from '${entry}';\n`,
  );
  writeFileSync(join(destination, "entry.d.ts"), `export * from '${entry}';\n`);
  if (target === "browser") {
    writeFileSync(
      join(destination, "pure.js"),
      "import './sdk-pure-contract-check.js';\nexport * from './typescript-pure/index.js';\n",
    );
    writeFileSync(
      join(destination, "pure.d.ts"),
      "export * from './typescript-pure/index.js';\n",
    );
  }
  const manifest = {
    name: target === "node" ? "xmtp-sdk" : "xmtp-sdk-browser",
    version: "0.0.0-stage",
    private: true,
    type: "module",
    engines: { node: ">=22.12.0" },
    exports: {
      ".": { types: "./entry.d.ts", import: "./entry.js" },
      ...(target === "browser"
        ? { "./pure": { types: "./pure.d.ts", import: "./pure.js" } }
        : {}),
    },
    ...(target === "node"
      ? { imports: { "#xmtp/binding": "./xmtp_sdk.js" } }
      : {}),
    dependencies: Object.fromEntries(
      runtimes.map((name) => [
        `@ubjs/${name}`,
        JSON.parse(
          readFileSync(
            join(destination, "node_modules/@ubjs", name, "package.json"),
          ),
        ).version,
      ]),
    ),
    bundledDependencies: runtimes.map((name) => `@ubjs/${name}`),
  };
  writeFileSync(
    join(destination, "package.json"),
    JSON.stringify(manifest, null, 2) + "\n",
  );
  if (target === "browser") {
    const bindings = {
      "@ubjs/core": join(
        destination,
        "node_modules/@ubjs/core/dist/esm/index.js",
      ),
      "@ubjs/wasm": join(
        destination,
        "node_modules/@ubjs/wasm/dist/browser/src/index.js",
      ),
      "@ubjs/wasm/core": join(
        destination,
        "node_modules/@ubjs/wasm/dist/core/src/index.js",
      ),
      "@ubjs/wasm/browser": join(
        destination,
        "node_modules/@ubjs/wasm/dist/browser/src/index.js",
      ),
    };
    function rewrite(directory) {
      for (const path of files(directory).filter((item) =>
        item.endsWith(".js"),
      )) {
        let source = readFileSync(path, "utf8");
        for (const [name, entry] of Object.entries(bindings)) {
          let specifier = relative(dirname(path), entry).replaceAll("\\", "/");
          if (!specifier.startsWith(".")) specifier = `./${specifier}`;
          source = source
            .replaceAll(`"${name}"`, JSON.stringify(specifier))
            .replaceAll(`'${name}'`, JSON.stringify(specifier));
        }
        if (!path.includes("node_modules")) {
          const tree = path.includes("typescript-pure")
            ? "typescript-pure"
            : "typescript-wasm";
          let binding = relative(
            dirname(path),
            join(destination, tree, "xmtp_sdk.js"),
          );
          if (!binding.startsWith(".")) binding = `./${binding}`;
          source = source
            .replaceAll('"#xmtp/binding"', JSON.stringify(binding))
            .replaceAll("'#xmtp/binding'", JSON.stringify(binding));
        }
        writeFileSync(path, source);
      }
    }
    rewrite(destination);
    for (const name of runtimes)
      rewrite(join(destination, "node_modules/@ubjs", name));
  }
  const contract = contracts[0].contract;
  const assets = Object.fromEntries(
    files(destination)
      .filter((path) => !path.endsWith(".d.ts"))
      .map((path) => [relative(destination, path), hash(path)]),
  );
  {
    // npm removes files such as package-lock.json from bundled dependencies.
    // Check the runtime files that npm ships, including its loaders and binaries.
    const npmCandidates = [
      process.env.XMTP_SDK_NPM_CLI
        ? resolve(process.env.XMTP_SDK_NPM_CLI)
        : undefined,
      join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"),
      resolve(
        dirname(process.execPath),
        "../lib/node_modules/npm/bin/npm-cli.js",
      ),
    ];
    for (const directory of (process.env.PATH ?? "").split(delimiter)) {
      const link = join(directory, "npm");
      if (existsSync(link)) {
        const cli = realpathSync(link);
        if (cli.endsWith("npm-cli.js")) npmCandidates.push(cli);
      }
    }
    const npmCli = npmCandidates.find((path) => path && existsSync(path));
    if (!npmCli) throw new Error("SDK package staging cannot find npm-cli.js");
    const packed = JSON.parse(
      execFileSync(
        process.execPath,
        [
          npmCli,
          "pack",
          realpathSync(destination),
          "--dry-run",
          "--json",
          "--ignore-scripts",
        ],
        { cwd: realpathSync(destination), encoding: "utf8" },
      ),
    )[0];
    const shipped = new Set(packed.files.map((file) => file.path));
    for (const name of runtimes) {
      const prefix = `node_modules/@ubjs/${name}/`;
      if (
        !packed.bundled.includes(`@ubjs/${name}`) ||
        !shipped.has(prefix + "package.json")
      )
        throw new Error(`SDK package omits runtime: ${name}`);
      const runtimeFiles = [...shipped].filter((path) =>
        path.startsWith(prefix),
      );
      const runtimeManifest = JSON.parse(
        readFileSync(join(destination, prefix, "package.json")),
      );
      const entries = [];
      function collectEntries(value) {
        if (typeof value === "string") entries.push(value);
        else if (Array.isArray(value)) value.forEach(collectEntries);
        else if (value && typeof value === "object") {
          for (const [condition, entry] of Object.entries(value)) {
            if (condition !== "types" && condition !== "require")
              collectEntries(entry);
          }
        }
      }
      if (runtimeManifest.exports !== undefined)
        collectEntries(runtimeManifest.exports);
      else
        entries.push(
          runtimeManifest.module ?? runtimeManifest.main ?? "index.js",
        );
      if (entries.length === 0)
        throw new Error(`SDK package has no runtime entry: ${name}`);
      for (const entry of entries) {
        const required = relative(
          destination,
          resolve(destination, prefix, entry),
        ).replaceAll("\\", "/");
        if (!required.startsWith(prefix) || !shipped.has(required))
          throw new Error(
            `SDK package omits required runtime entry: ${required}`,
          );
      }
      if (!runtimeFiles.some((path) => path.endsWith(".js")))
        throw new Error(`SDK package omits runtime loader: ${name}`);
      if (
        name === "node" &&
        !runtimeFiles.some((path) => path.endsWith(".node"))
      )
        throw new Error("SDK package omits native runtime binary");
      for (const path of runtimeFiles) {
        if (!path.endsWith(".d.ts"))
          assets[path] = hash(join(destination, path));
      }
    }
  }
  writeFileSync(
    join(destination, "sdk-contract.json"),
    JSON.stringify(
      {
        contract,
        generator: contracts[0].generator,
        proof_origin: contracts[0].proof_origin,
        final_gate: contracts[0].final_gate,
        assets,
      },
      null,
      2,
    ) + "\n",
  );
  if (target === "node") {
    writeFileSync(
      join(destination, "sdk-contract-check.js"),
      `
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
const metadata = JSON.parse(readFileSync(new URL('./sdk-contract.json', import.meta.url)));
if (metadata.contract !== '${contract}' || metadata.generator !== '${contracts[0].generator}') throw new Error('SDK contract mismatch');
for (const [path, expected] of Object.entries(metadata.assets)) {
  const actual = createHash('sha256').update(readFileSync(new URL(path, import.meta.url))).digest('hex');
  if (actual !== expected) throw new Error('SDK asset mismatch: ' + path);
}
`.trim() + "\n",
    );
  } else {
    // Bundlers can transform JS modules. Check the bytes of the WASM assets
    // through static URLs, so bundlers can copy and rename each binary.
    const browserCheck = (selected) => `
const metadata = await (await fetch(new URL('./sdk-contract.json', import.meta.url))).json();
if (metadata.contract !== '${contract}' || metadata.generator !== '${contracts[0].generator}') throw new Error('SDK contract mismatch');
const assets = [${selected.map((path) => `{path: ${JSON.stringify(path)}, expected: ${JSON.stringify(assets[path])}, url: new URL(${JSON.stringify(`./${path}`)}, import.meta.url)}`).join(",")}];
await Promise.all(assets.map(async ({ path, expected, url }) => {
  const response = await fetch(url);
  if (!response.ok) throw new Error('SDK missing asset: ' + path);
  const hash = await crypto.subtle.digest('SHA-256', await response.arrayBuffer());
  const actual = [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  if (actual !== expected || metadata.assets[path] !== expected) throw new Error('SDK asset mismatch: ' + path);
}));
`;
    writeFileSync(
      join(destination, "sdk-contract-check.js"),
      browserCheck([
        "typescript-wasm/xmtp_sdk.wasm",
        "typescript-pure/xmtp_sdk.wasm",
      ]).trim() + "\n",
    );
    writeFileSync(
      join(destination, "sdk-pure-contract-check.js"),
      browserCheck(["typescript-pure/xmtp_sdk.wasm"]).trim() + "\n",
    );
  }
  // Keep the prior product until compilation and all package checks succeed.
  if (existsSync(product)) renameSync(product, previous);
  try {
    renameSync(destination, product);
  } catch (error) {
    if (existsSync(previous)) {
      try {
        renameSync(previous, product);
      } catch (rollbackError) {
        throw new AggregateError(
          [error, rollbackError],
          `Previous SDK product remains at ${previous}`,
        );
      }
    }
    throw error;
  }
  rmSync(previous, { recursive: true, force: true });
  console.log(`SDK staged ${target} ${contract} at ${product}`);
} finally {
  // Preserve the backup if a filesystem error prevents rollback.
  if (!existsSync(previous)) rmSync(staging, { recursive: true, force: true });
}
