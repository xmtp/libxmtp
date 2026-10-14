import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";

// npm's JavaScript CLI, to run through Node: Windows npm.cmd is a shell entry,
// and Nix can install npm apart from the Node executable, with an npm link to
// npm-cli.js. Read that link; never run it as a child process.
export function npmCli() {
  const candidates = [
    process.env.XMTP_SDK_NPM_CLI
      ? resolve(process.env.XMTP_SDK_NPM_CLI)
      : undefined,
    join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"),
    resolve(dirname(process.execPath), "../lib/node_modules/npm/bin/npm-cli.js"),
  ];
  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    const link = join(directory, "npm");
    if (existsSync(link)) {
      const cli = realpathSync(link);
      if (cli.endsWith("npm-cli.js")) candidates.push(cli);
    }
  }
  const cli = candidates.find((path) => path && existsSync(path));
  if (!cli) throw new Error("SDK packaging cannot find npm-cli.js");
  return cli;
}

// The published Node SDK supports every Node platform in the platform list.
const platformList = JSON.parse(
  readFileSync(new URL("../../../nix/sdk-platforms.json", import.meta.url)),
);
export const NODE_PLATFORMS = Object.freeze(
  Object.fromEntries(
    platformList.node.map(({ target, rustTarget, library, nixSystems }) => [
      target,
      [rustTarget, library, nixSystems.length === 0],
    ]),
  ),
);
const hash = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

// A Nix pair names the SDK closure identity, which must equal the binding's.
// A cargo pair (no Nix system builds it) names the source key of its SDK
// inputs, which must be the release commit's, and the commit that built it:
// a cache restore ships bytes that an earlier commit with the same key built.
export function stageNodePlatforms(
  input,
  output,
  binding,
  runtimeRevision,
  packageName,
  releaseSource,
) {
  const platforms = {};
  const exports = {};
  for (const [target, [rustTarget, library, cargoBuilt]] of Object.entries(
    NODE_PLATFORMS,
  )) {
    const root = join(input, target);
    const receipt = JSON.parse(
      readFileSync(join(root, "sdk-node-platform.json")),
    );
    if (
      receipt.schema !== 1 ||
      receipt.target !== target ||
      receipt.rustTarget !== rustTarget ||
      (cargoBuilt
        ? receipt.source !== releaseSource ||
          !/^[0-9a-f]{40}$/.test(receipt.commit ?? "")
        : receipt.source !== binding.artifact.source || "commit" in receipt) ||
      receipt.features !== binding.artifact.features ||
      receipt.profile !== "release" ||
      binding.artifact.profile !== "release" ||
      receipt.runtimeRevision !== runtimeRevision
    ) {
      throw new Error(`SDK Node platform provenance mismatch: ${target}`);
    }
    const addon = `uniffi-runtime-napi.${target}.node`;
    if (
      Object.keys(receipt.files).sort().join("\n") !==
      [library, addon].sort().join("\n")
    )
      throw new Error(`SDK Node platform asset set mismatch: ${target}`);
    for (const name of [library, addon]) {
      if (hash(join(root, name)) !== receipt.files[name])
        throw new Error(`SDK Node platform asset mismatch: ${target}/${name}`);
    }
    const native = join(output, "native", target);
    mkdirSync(native, { recursive: true });
    copyFileSync(join(root, library), join(native, library));
    writeFileSync(
      join(native, "package.json"),
      JSON.stringify({
        name: `${packageName}-native-${target}`,
        private: true,
      }),
    );
    copyFileSync(
      join(root, addon),
      join(output, "node_modules/@ubjs/node", addon),
    );
    exports[`./native/${target}/package.json`] =
      `./native/${target}/package.json`;
    platforms[target] = receipt;
  }
  // npm honors the runtime's files allowlist. Include every pinned addon.
  const manifestFile = join(output, "node_modules/@ubjs/node/package.json");
  const manifest = JSON.parse(readFileSync(manifestFile));
  if (manifest.files)
    manifest.files = [
      ...new Set([
        ...manifest.files,
        ...Object.keys(NODE_PLATFORMS).map(
          (target) => `uniffi-runtime-napi.${target}.node`,
        ),
      ]),
    ];
  writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + "\n");
  return { platforms, exports };
}

// Use the pinned runtime's package resolver and platform detection. The public
// package exports only supported targets. An unsupported target cannot resolve.
export function usePlatformPackages(source, packageName) {
  const call =
    /resolveLibPath\(\{\s*crateName:\s*["']xmtp_sdk["'],\s*callerUrl:\s*import\.meta\.url,?\s*\}\)/g;
  const matches = [...source.matchAll(call)];
  if (matches.length !== 1)
    throw new Error("SDK Node library resolver changed");
  return source.replace(
    call,
    `resolveLibPath({crateName: "xmtp_sdk", callerUrl: import.meta.url, npmPackageBase: ${JSON.stringify(packageName + "/native/")}, tripleStyle: "node"})`,
  );
}
