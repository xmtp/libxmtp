import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// The published Node SDK keeps the existing six-target support matrix.
export const NODE_PLATFORMS = Object.freeze({
  "darwin-arm64": ["aarch64-apple-darwin", "libxmtp_sdk.dylib"],
  "linux-x64-gnu": ["x86_64-unknown-linux-gnu", "libxmtp_sdk.so"],
  "linux-x64-musl": ["x86_64-unknown-linux-musl", "libxmtp_sdk.so"],
  "linux-arm64-gnu": ["aarch64-unknown-linux-gnu", "libxmtp_sdk.so"],
  "linux-arm64-musl": ["aarch64-unknown-linux-musl", "libxmtp_sdk.so"],
  "win32-x64-msvc": ["x86_64-pc-windows-msvc", "xmtp_sdk.dll"],
});
const hash = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

export function stageNodePlatforms(
  input,
  output,
  binding,
  runtimeRevision,
  packageName,
) {
  const platforms = {};
  const exports = {};
  for (const [target, [rustTarget, library]] of Object.entries(
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
      receipt.source !== binding.artifact.source ||
      receipt.generator !== binding.generator ||
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
