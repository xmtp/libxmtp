#!/usr/bin/env node
// Build only JavaScript, declarations, and native adapter sources. Native slices
// join this immutable component in package.py on the assembly host.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  symlinkSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { checkGeneratedAssets } from "../../../crates/xmtp_sdk/dev/check-generated-assets.mjs";
import {
  files as packageFiles,
  hash,
  makeWritable,
  readJson,
  rewriteImports,
  stagePackage,
  writeEntry,
  writeJson,
} from "../../../crates/xmtp_sdk/dev/package-staging.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const options = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i];
  if (
    !["--generated", "--runtime", "--out"].includes(key) ||
    !process.argv[i + 1]
  )
    throw new Error(`unknown or incomplete argument: ${key}`);
  options[key.slice(2)] = resolve(process.argv[i + 1]);
}
const generated =
  options.generated ??
  resolve(
    process.env.XMTP_SDK_GENERATED_DIR ?? join(root, "target/sdk-generated"),
  );
const output =
  options.out ??
  join(
    resolve(
      process.env.XMTP_SDK_PACKAGES_DIR ?? join(root, "target/sdk-packages"),
    ),
    "react-native",
  );
const runtime =
  options.runtime ??
  process.env.XMTP_SDK_RUNTIME_DIR ??
  execFileSync(
    "nix",
    ["build", "--no-link", "--print-out-paths", ".#ubjs-react-native"],
    { cwd: root, encoding: "utf8" },
  ).trim();
const tree = join(generated, "typescript-react-native");
const receipt = readJson(join(tree, "sdk-contract.json"));
checkGeneratedAssets(generated, "typescript-react-native", receipt);
const declaredSource = process.env.XMTP_SDK_EXPECTED_SOURCE;
const declaredRender = process.env.XMTP_SDK_EXPECTED_RENDER;
if (Boolean(declaredSource) !== Boolean(declaredRender))
  throw new Error(
    "RN staging requires both expected source and render identities",
  );
const [source, render] =
  declaredSource && declaredRender
    ? [declaredSource, declaredRender]
    : execFileSync(
        "python3",
        [
          join(root, "crates/xmtp_sdk/dev/sdk-artifacts.py"),
          "provenance",
          "--targets",
          "react-native",
        ],
        { cwd: root, encoding: "utf8" },
      )
        .trim()
        .split(/\s+/);
if (receipt.artifact?.source !== source || receipt.generator !== render)
  throw new Error(
    "RN generated sources are stale: run just sdk generate react-native",
  );
if (
  receipt.artifact?.features !== "" ||
  receipt.artifact?.profile !== "release"
)
  throw new Error("RN JS requires default-feature release bindings");
const files = (directory) =>
  packageFiles(directory, true).map((path) =>
    relative(directory, path).replaceAll("\\", "/"),
  );
const runtimeReceipt = readJson(join(runtime, "runtime-provenance.json"));
const forkRevision = readFileSync(join(root, "Cargo.toml"), "utf8").match(
  /\[workspace\.metadata\.xmtp-sdk-fork\][\s\S]*?rev\s*=\s*"([a-f0-9]+)"/,
)?.[1];
if (
  !forkRevision ||
  runtimeReceipt.schema !== 1 ||
  runtimeReceipt.forkRevision !== forkRevision ||
  receipt.forkRevision !== forkRevision
)
  throw new Error("RN generator/runtime fork revision mismatch");
const runtimeFiles = files(runtime)
  .filter((name) => name !== "runtime-provenance.json")
  .sort();
if (
  JSON.stringify(runtimeFiles) !==
  JSON.stringify(Object.keys(runtimeReceipt.assets ?? {}).sort())
)
  throw new Error("RN runtime asset set mismatch");
for (const name of runtimeFiles)
  if (hash(join(runtime, name)) !== runtimeReceipt.assets[name])
    throw new Error(`RN runtime asset mismatch: ${name}`);
const manifest = readJson(join(root, "sdks/react-native/package.json"));
stagePackage(output, (destination, temporary) => {
  const compile = join(temporary, "compile");
  mkdirSync(join(destination, "dist/runtime"), { recursive: true });
  cpSync(join(root, "LICENSE"), join(destination, "LICENSE"));
  mkdirSync(join(destination, "THIRD_PARTY_LICENSES"), { recursive: true });
  cpSync(
    join(runtime, "LICENSE"),
    join(destination, "THIRD_PARTY_LICENSES/ubrn-LICENSE"),
  );
  const core = existsSync(join(runtime, "core/package.json"))
    ? join(runtime, "core")
    : join(runtime, "lib/node_modules/@ubjs/core");
  cpSync(core, join(destination, "dist/runtime/core"), {
    recursive: true,
    dereference: true,
  });
  cpSync(join(runtime, "cpp/includes"), join(destination, "cpp/runtime"), {
    recursive: true,
    dereference: true,
  });
  cpSync(tree, compile, {
    recursive: true,
    dereference: true,
    filter: (path) =>
      !relative(tree, path).split(/[\\/]/).includes("node_modules"),
  });
  makeWritable(destination);
  makeWritable(compile);
  const compiler =
    process.env.XMTP_SDK_TSC ?? join(root, "node_modules/.bin/tsc");
  const compilerVersion = execFileSync(compiler, ["--version"], {
    cwd: root,
    encoding: "utf8",
  }).trim();
  if (compilerVersion !== "Version 7.0.2")
    throw new Error(
      `RN package requires TypeScript 7.0.2, got ${compilerVersion}`,
    );
  mkdirSync(join(temporary, "node_modules/@ubjs"), { recursive: true });
  symlinkSync(core, join(temporary, "node_modules/@ubjs/core"));
  const tsconfig = join(temporary, "tsconfig.json");
  writeJson(tsconfig, {
    compilerOptions: {
      // Stock Metro lowers class fields but does not transform static blocks.
      target: "ES2020",
      useDefineForClassFields: true,
      module: "ESNext",
      moduleResolution: "Bundler",
      jsx: "react-jsx",
      strict: true,
      skipLibCheck: true,
      allowImportingTsExtensions: true,
      types: [],
      paths: {
        "react-native": [
          join(
            process.env.XMTP_SDK_REACT_NATIVE_TYPES ??
              join(root, "sdks/react-native/node_modules/react-native"),
            "types_generated/index.d.ts",
          ),
        ],
      },
      lib: ["ES2022", "DOM", "DOM.Iterable"],
    },
  });
  const compileTree = (source, outDir, entries, react = false) => {
    const config = join(
      temporary,
      react ? "react.tsconfig.json" : "sdk.tsconfig.json",
    );
    const base = readJson(tsconfig);
    writeJson(config, {
      ...base,
      compilerOptions: {
        ...base.compilerOptions,
        declaration: true,
        outDir,
        rootDir: source,
        rewriteRelativeImportExtensions: true,
        allowImportingTsExtensions: true,
      },
      files: entries,
    });
    execFileSync(compiler, ["--project", config], {
      cwd: root,
      stdio: "inherit",
    });
  };
  compileTree(
    compile,
    join(destination, "dist"),
    files(compile)
      .filter(
        (name) =>
          name.endsWith(".ts") &&
          !name.endsWith(".d.ts") &&
          !name.endsWith(".test.ts"),
      )
      .map((name) => join(compile, name)),
  );
  cpSync(join(tree, "jsi"), join(destination, "cpp/generated"), {
    recursive: true,
  });
  writeEntry(join(destination, "dist"), "entry", "./index.js");
  const coreManifest = readJson(join(core, "package.json"));
  const coreEntry =
    coreManifest.module ??
    coreManifest.exports?.["."]?.import ??
    coreManifest.main;
  if (
    typeof coreEntry !== "string" ||
    !existsSync(resolve(destination, "dist/runtime/core", coreEntry))
  )
    throw new Error("RN core runtime has no ESM entry");
  const coreTypes = coreManifest.types ?? coreManifest.exports?.["."]?.types;
  if (
    typeof coreTypes !== "string" ||
    !existsSync(resolve(destination, "dist/runtime/core", coreTypes))
  )
    throw new Error("RN core runtime has no declaration entry");
  for (const name of files(join(destination, "dist"))) {
    if (!/\.(?:js|ts)$/.test(name)) continue;
    const path = join(destination, "dist", name);
    rewriteImports(path, {
      "@ubjs/core": resolve(destination, "dist/runtime/core", coreEntry),
      "#xmtp/binding": join(destination, "dist/xmtp_sdk.js"),
    });
  }
  for (const name of ["android", "ios", "cpp", "src/specs"]) {
    const path = join(root, "sdks/react-native", name);
    if (existsSync(path))
      cpSync(path, join(destination, name), {
        recursive: true,
        filter: (path) =>
          !relative(join(root, "sdks/react-native", name), path)
            .split(/[\\/]/)
            .some((part) =>
              ["build", "Pods", ".gradle", "jniLibs", "libs"].includes(part),
            ),
      });
  }
  if (!existsSync(join(destination, "src/specs/NativeXmtpReactNative.ts")))
    throw new Error("RN package omits its private TurboModule codegen spec");
  for (const name of readdirSync(join(root, "sdks/react-native")).filter(
    (name) =>
      name.endsWith(".podspec") ||
      ["react-native.config.cjs", "README.md"].includes(name),
  ))
    cpSync(join(root, "sdks/react-native", name), join(destination, name));
  delete manifest.scripts;
  delete manifest.devDependencies;
  delete manifest.files;
  delete manifest.exports["./react"];
  const react = join(root, "sdks/react-native/src/react");
  if (existsSync(join(react, "index.ts"))) {
    // Resolve the generated root while emitting the optional source entry.
    const reactConfig = readJson(tsconfig);
    reactConfig.compilerOptions.paths = {
      ...reactConfig.compilerOptions.paths,
      "@xmtp/react-native-sdk": [join(destination, "dist/entry.d.ts")],
    };
    writeJson(tsconfig, reactConfig);
    if (process.env.XMTP_SDK_REACT_TYPES) {
      reactConfig.compilerOptions.paths.react = [
        join(process.env.XMTP_SDK_REACT_TYPES, "index.d.ts"),
      ];
      reactConfig.compilerOptions.paths["react/*"] = [
        join(process.env.XMTP_SDK_REACT_TYPES, "*"),
      ];
      writeJson(tsconfig, reactConfig);
    } else {
      symlinkSync(join(root, "node_modules"), join(compile, "node_modules"));
    }
    compileTree(
      react,
      join(destination, "dist/react"),
      files(react)
        .filter((name) => /\.tsx?$/.test(name) && !name.endsWith(".d.ts"))
        .map((name) => join(react, name)),
      true,
    );
    manifest.exports["./react"] = {
      types: "./dist/react/index.d.ts",
      "react-native": "./dist/react/index.js",
      import: "./dist/react/index.js",
      default: "./dist/react/index.js",
    };
  }
  writeJson(join(destination, "package.json"), manifest, true);
  const stagedAssets = Object.fromEntries(
    files(destination)
      .sort()
      .map((name) => [name, hash(join(destination, name))]),
  );
  const identity = {
    productContract: createHash("sha256")
      .update(JSON.stringify(stagedAssets))
      .digest("hex"),
    javascriptToolchain: "typescript-7.0.2-es2020-esnext-define-v1",
    source: receipt.artifact.source,
    generator: receipt.generator,
    profile: receipt.artifact.profile,
    features: receipt.artifact.features,
    forkRevision,
    renderContract: createHash("sha256")
      .update(
        JSON.stringify(
          Object.fromEntries(
            Object.entries(receipt.files).sort(([a], [b]) =>
              a < b ? -1 : a > b ? 1 : 0,
            ),
          ),
        ),
      )
      .digest("hex"),
    runtimeContract: hash(join(runtime, "runtime-provenance.json")),
  };
  writeJson(
    join(destination, "sdk-contract.json"),
    {
      schema: 1,
      kind: "react-native-js",
      identity,
      generated: receipt,
      runtime: runtimeReceipt,
      assets: stagedAssets,
    },
    true,
  );
});
console.log(`React Native JS/types staged: ${output}`);
