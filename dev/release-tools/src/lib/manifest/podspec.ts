import fs from "node:fs";
import path from "node:path";
import type { ManifestProvider } from "../../types";

const PODSPEC_VERSION_REGEX = /(spec\.version\s*=\s*)"([^"]+)"/;

export function readPodspecVersion(podspecPath: string): string {
  const content = fs.readFileSync(podspecPath, "utf-8");
  const match = content.match(PODSPEC_VERSION_REGEX);
  if (!match) {
    throw new Error(`Could not find spec.version in ${podspecPath}`);
  }
  return match[2];
}

export function writePodspecVersion(
  podspecPath: string,
  version: string,
): void {
  const content = fs.readFileSync(podspecPath, "utf-8");
  if (!PODSPEC_VERSION_REGEX.test(content)) {
    throw new Error(`Could not find spec.version in ${podspecPath}`);
  }
  const updated = content.replace(PODSPEC_VERSION_REGEX, `$1"${version}"`);
  fs.writeFileSync(podspecPath, updated);
}

export function createPodspecManifestProvider(
  relativePath: string,
): ManifestProvider {
  return {
    readVersion: (repoRoot) =>
      readPodspecVersion(path.join(repoRoot, relativePath)),
    writeVersion: (repoRoot, version) =>
      writePodspecVersion(path.join(repoRoot, relativePath), version),
  };
}
