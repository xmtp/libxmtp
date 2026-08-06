import fs from "node:fs";
import path from "node:path";
import { applyEdits, modify } from "jsonc-parser";
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

/**
 * Rewrite a single dependency's version spec in a package.json file,
 * preserving all formatting and other fields via jsonc-parser.
 *
 * Throws if the dependency is not found in the `dependencies` object.
 */
export function setPackageJsonDependency(
  packageJsonPath: string,
  depName: string,
  version: string,
): void {
  const source = fs.readFileSync(packageJsonPath, "utf-8");
  const parsed = JSON.parse(source) as {
    dependencies?: Record<string, string>;
  };

  if (!parsed.dependencies || !(depName in parsed.dependencies)) {
    throw new Error(
      `Dependency ${depName} not found in dependencies of ${packageJsonPath}`,
    );
  }

  const formattingOptions = {
    insertSpaces: true,
    tabSize: detectTabSize(source),
  };

  const edits = modify(source, ["dependencies", depName], version, {
    formattingOptions,
  });
  const result = applyEdits(source, edits);
  fs.writeFileSync(packageJsonPath, result);
}

/**
 * Detect the indent width used in an existing JSON source by looking at the
 * first indented `"key":` line. Falls back to 2 spaces.
 */
function detectTabSize(source: string): number {
  const match = source.match(/\n( +)"/);
  return match && match[1].length > 0 ? match[1].length : 2;
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
