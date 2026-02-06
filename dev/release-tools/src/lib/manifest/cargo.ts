import fs from "node:fs";
import path from "node:path";
import { parse } from "smol-toml";
import type { ManifestProvider } from "../../types";
import { execSilent } from "../exec";

const WORKSPACE_VERSION_REGEX =
  /(\[workspace\.package\][\s\S]*?version\s*=\s*)"([^"]+)"/;

export function readCargoVersion(cargoTomlPath: string): string {
  const content = fs.readFileSync(cargoTomlPath, "utf-8");
  const parsed = parse(content) as {
    workspace?: { package?: { version?: string } };
  };
  const version = parsed.workspace?.package?.version;
  if (!version) {
    throw new Error(
      `Could not find workspace.package.version in ${cargoTomlPath}`,
    );
  }
  return version;
}

export function writeCargoVersion(
  cargoTomlPath: string,
  version: string,
  repoRoot: string,
): void {
  const content = fs.readFileSync(cargoTomlPath, "utf-8");
  if (!WORKSPACE_VERSION_REGEX.test(content)) {
    throw new Error(
      `Could not find workspace.package.version in ${cargoTomlPath}`,
    );
  }
  const updated = content.replace(WORKSPACE_VERSION_REGEX, `$1"${version}"`);
  fs.writeFileSync(cargoTomlPath, updated);

  // Format with taplo (best-effort)
  try {
    execSilent(`taplo format ${cargoTomlPath}`, repoRoot);
  } catch {
    // taplo may not be installed; skip formatting
  }

  // Refresh Cargo.lock (best-effort)
  try {
    execSilent("cargo update --workspace", repoRoot);
  } catch {
    // cargo may not be available or project may not build; skip lockfile refresh
  }
}

export function createCargoManifestProvider(
  relativePath: string,
): ManifestProvider {
  return {
    readVersion: (repoRoot) =>
      readCargoVersion(path.join(repoRoot, relativePath)),
    writeVersion: (repoRoot, version) =>
      writeCargoVersion(path.join(repoRoot, relativePath), version, repoRoot),
  };
}
