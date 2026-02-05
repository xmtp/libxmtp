import fs from "node:fs";
import path from "node:path";
import type { ManifestProvider } from "../types.js";

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

const GRADLE_VERSION_REGEX = /^version=(.+)$/m;

export function readGradlePropertiesVersion(propsPath: string): string {
  const content = fs.readFileSync(propsPath, "utf-8");
  const match = content.match(GRADLE_VERSION_REGEX);
  if (!match) {
    throw new Error(`Could not find version= in ${propsPath}`);
  }
  return match[1];
}

export function writeGradlePropertiesVersion(
  propsPath: string,
  version: string,
): void {
  let content = fs.readFileSync(propsPath, "utf-8");
  if (GRADLE_VERSION_REGEX.test(content)) {
    content = content.replace(GRADLE_VERSION_REGEX, `version=${version}`);
  } else {
    content += `version=${version}\n`;
  }
  fs.writeFileSync(propsPath, content);
}

export function createGradlePropertiesManifestProvider(
  relativePath: string,
): ManifestProvider {
  return {
    readVersion: (repoRoot) =>
      readGradlePropertiesVersion(path.join(repoRoot, relativePath)),
    writeVersion: (repoRoot, version) =>
      writeGradlePropertiesVersion(path.join(repoRoot, relativePath), version),
  };
}
