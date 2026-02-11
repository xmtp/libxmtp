import fs from "node:fs";
import path from "node:path";
import type { ManifestProvider } from "../../types";

export function readPackageJsonVersion(packageJsonPath: string): string {
  const content = fs.readFileSync(packageJsonPath, "utf-8");
  const parsed = JSON.parse(content);
  if (!parsed.version) {
    throw new Error(`Could not find version in ${packageJsonPath}`);
  }
  return parsed.version;
}

export function writePackageJsonVersion(
  packageJsonPath: string,
  version: string,
): void {
  const content = fs.readFileSync(packageJsonPath, "utf-8");
  const parsed = JSON.parse(content);
  parsed.version = version;
  fs.writeFileSync(packageJsonPath, JSON.stringify(parsed, null, 2) + "\n");
}

export function createPackageJsonManifestProvider(
  relativePath: string,
): ManifestProvider {
  return {
    readVersion: (repoRoot) =>
      readPackageJsonVersion(path.join(repoRoot, relativePath)),
    writeVersion: (repoRoot, version) =>
      writePackageJsonVersion(path.join(repoRoot, relativePath), version),
  };
}
