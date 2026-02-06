import fs from "node:fs";
import path from "node:path";
import type { ManifestProvider } from "../../types";

const GRADLE_VERSION_REGEX = /^version\s*=\s*(.+)$/m;

export function readGradlePropertiesVersion(propsPath: string): string {
  const content = fs.readFileSync(propsPath, "utf-8");
  const match = content.match(GRADLE_VERSION_REGEX);
  if (!match) {
    throw new Error(`Could not find version= in ${propsPath}`);
  }
  return match[1].trim();
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
