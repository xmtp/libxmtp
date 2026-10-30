#!/usr/bin/env node
import { execFileSync } from "node:child_process";
// Compile matched generated trees and retain their runtime assets.
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  symlinkSync,
  readFileSync,
  realpathSync,
  rmSync,
  copyFileSync,
  cpSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { checkGeneratedAssets } from "./check-generated-assets.mjs";
import {
  npmCli,
  stageNodePlatforms,
  usePlatformPackages,
} from "./node-platforms.mjs";
import {
  files,
  hash,
  makeWritable,
  readJson,
  removeTree,
  rewriteImports,
  stagePackage,
  writeEntry,
  writeJson,
} from "./package-staging.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const generated = resolve(
  process.env.XMTP_SDK_GENERATED_DIR ?? "target/sdk-generated",
);
const output = resolve(
  process.env.XMTP_SDK_PACKAGES_DIR ?? "target/sdk-packages",
);
const target = process.argv[2];
const publicPackage = process.argv.includes("--public");
if (target === "react-native") {
  execFileSync(
    process.execPath,
    [join(root, "sdks/react-native/scripts/stage-package.mjs")],
    { cwd: root, stdio: "inherit" },
  );
  process.exit(0);
}
const platformDirectory = process.env.XMTP_SDK_NODE_PLATFORMS_DIR;
if (platformDirectory && (target !== "node" || !publicPackage))
  throw new Error("Node platform assembly requires a public Node product");
if (!["node", "browser"].includes(target))
  throw new Error("expected node or browser");
const sourceManifest = publicPackage
  ? readJson(join(root, "sdks", target, "package.json"))
  : undefined;
const trees =
  target === "node"
    ? ["typescript-napi"]
    : ["typescript-wasm", "typescript-pure"];
const contracts = trees.map((tree) =>
  readJson(join(generated, tree, "sdk-contract.json")),
);
// Each tree's contract digests its own library, so the browser's worker and
// pure trees match on the generator and the source, profile, and Rust target
// of their libraries instead.
if (
  contracts.some(
    (record) =>
      record.generator !== contracts[0].generator ||
      ["source", "profile", "target"].some(
        (field) => record.artifact?.[field] !== contracts[0].artifact?.[field],
      ),
  )
)
  throw new Error("SDK generated contract mismatch");
// target/sdk-generated is whatever the last generate wrote, so it must still
// match this checkout: the library's source identity and the render identity
// of its language. An explicit XMTP_SDK_GENERATED_DIR (a Nix output, or a
// release render) names its own trees. Windows reads its commit identity.
if (!process.env.XMTP_SDK_GENERATED_DIR) {
  const [source, render] = execFileSync(
    process.platform === "win32" ? "python" : "python3",
    [join(root, "crates/xmtp_sdk/dev/sdk-artifacts.py"), "provenance"],
    { cwd: root, encoding: "utf8" },
  )
    .trim()
    .split(/\s+/);
  trees.forEach((tree, i) => {
    if (
      contracts[i].artifact?.source !== source ||
      contracts[i].generator !== render
    )
      throw new Error(
        `${join(generated, tree)} was generated from other sources: run \`just sdk generate ${target}\``,
      );
  });
}
// A package ships only product libraries: no test exports such as
// `conformance` (the Nix fixtures) in any tree.
const productFeatures = {
  "typescript-napi": "",
  "typescript-wasm": "",
  "typescript-pure": "pure-only",
};
trees.forEach((tree, i) => {
  const features = contracts[i].artifact?.features;
  if (features !== productFeatures[tree])
    throw new Error(
      `SDK generated ${tree} has features ${features}, expected "${productFeatures[tree]}"`,
    );
});
for (let i = 0; i < trees.length; i++) {
  checkGeneratedAssets(generated, trees[i], contracts[i]);
}
const product = join(output, target);
const contract = stagePackage(product, (destination) => {
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
      // These paths are outside both runtime packages' files lists. Windows
      // npm installs can leave links to missing test build directories here.
      filter: (path) =>
        !relative(runtimeSource, path)
          .split(/[\\/]/)
          .some((part) => part === "tests" || part === "node_modules"),
    });
    const runtime = join(destination, "node_modules/@ubjs", name);
    makeWritable(runtime);
    rmSync(join(runtime, "dist/cjs"), { recursive: true, force: true });
    const manifestFile = join(runtime, "package.json");
    const manifest = readJson(manifestFile);
    if (manifest.exports?.["."]?.require) delete manifest.exports["."].require;
    if (manifest.module) manifest.main = manifest.module;
    writeJson(manifestFile, manifest, true);
  }
  let nodePlatforms;
  if (platformDirectory) {
    // The release commit's source key and runtime fork revision, from the
    // one reader of both.
    const release = JSON.parse(
      execFileSync(
        "python3",
        [join(root, "crates/xmtp_sdk/dev/sdk-artifacts.py"), "key"],
        { cwd: root, encoding: "utf8" },
      ),
    );
    nodePlatforms = stageNodePlatforms(
      resolve(platformDirectory),
      destination,
      contracts[0],
      release.forkRevision,
      sourceManifest.name,
      release.source,
    );
  }
  const compile = mkdtempSync(join(output, ".sdk-compile-"));
  try {
    for (const tree of trees) {
      cpSync(join(generated, tree), join(compile, tree), {
        recursive: true,
        filter: (path) => !path.split(/[\\/]/).includes("node_modules"),
      });
      // The compile step rewrites manifests and sources in this copy.
      makeWritable(join(compile, tree));
    }
    if (nodePlatforms) {
      const ffi = join(compile, "typescript-napi/xmtp_sdk-ffi.ts");
      writeFileSync(
        ffi,
        usePlatformPackages(readFileSync(ffi, "utf8"), sourceManifest.name),
      );
    }
    symlinkSync(
      join(destination, "node_modules"),
      join(compile, "node_modules"),
    );
    const tsconfig = join(compile, "tsconfig.json");
    writeJson(tsconfig, {
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
    });
    for (const tree of trees) {
      const source = join(compile, tree);
      const sourceManifest = join(source, "package.json");
      writeJson(sourceManifest, {
        ...readJson(sourceManifest),
        type: "module",
      });
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
      writeJson(
        join(dest, "package.json"),
        {
          private: true,
          type: "module",
          imports: { "#xmtp/binding": "./xmtp_sdk.js" },
        },
        true,
      );
    }
  } finally {
    removeTree(compile);
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
  // Public entries expose the compiled package roots.
  writeEntry(destination, "entry", entry);
  if (target === "browser") {
    writeEntry(destination, "pure", "./typescript-pure/index.js");
  }
  const manifest = {
    name: target === "node" ? "xmtp-sdk" : "xmtp-sdk-browser",
    version: readJson(join(root, "sdks", target, "package.json")).version,
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
        readJson(join(destination, "node_modules/@ubjs", name, "package.json"))
          .version,
      ]),
    ),
    bundledDependencies: runtimes.map((name) => `@ubjs/${name}`),
  };
  if (sourceManifest) {
    for (const field of [
      "name",
      "version",
      "description",
      "keywords",
      "homepage",
      "bugs",
      "license",
      "author",
      "repository",
      "publishConfig",
    ]) {
      if (sourceManifest[field] !== undefined)
        manifest[field] = sourceManifest[field];
    }
    delete manifest.private;
    manifest.main = "./entry.js";
    manifest.types = "./entry.d.ts";
    manifest.exports["./package.json"] = "./package.json";
    Object.assign(manifest.exports, nodePlatforms?.exports);
  }
  writeFileSync(
    join(destination, "package.json"),
    JSON.stringify(manifest, null, 2),
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
        const tree = path.includes("typescript-pure")
          ? "typescript-pure"
          : "typescript-wasm";
        rewriteImports(path, {
          ...bindings,
          ...(!path.includes("node_modules")
            ? { "#xmtp/binding": join(destination, tree, "xmtp_sdk.js") }
            : {}),
        });
      }
    }
    rewrite(destination);
    for (const name of runtimes)
      rewrite(join(destination, "node_modules/@ubjs", name));
  }
  const contract =
    contracts.length === 1
      ? contracts[0].contract
      : createHash("sha256")
          .update(contracts.map((record) => record.contract).join("\n"))
          .digest("hex");
  const assets = Object.fromEntries(
    files(destination)
      .filter((path) => !path.endsWith(".d.ts"))
      .map((path) => [relative(destination, path), hash(path)]),
  );
  {
    // npm removes files such as package-lock.json from bundled dependencies.
    // Check the runtime files that npm ships, including its loaders and binaries.
    const npm = npmCli();
    const packed = JSON.parse(
      execFileSync(
        process.execPath,
        [
          npm,
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
      const runtimeManifest = readJson(
        join(destination, prefix, "package.json"),
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
  writeJson(
    join(destination, "sdk-contract.json"),
    {
      contract,
      generator: contracts[0].generator,
      proof_origin: contracts[0].proof_origin,
      final_gate: contracts[0].final_gate,
      platform_scope: nodePlatforms
        ? "supported-matrix"
        : target === "node"
          ? "development-host"
          : "browser",
      platforms: nodePlatforms?.platforms,
      assets,
    },
    true,
  );
  return contract;
});
console.log(`SDK staged ${target} ${contract} at ${product}`);
