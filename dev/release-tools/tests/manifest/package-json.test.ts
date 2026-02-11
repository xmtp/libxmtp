import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import {
  readPackageJsonVersion,
  writePackageJsonVersion,
} from "../../src/lib/manifest";

const SAMPLE_PACKAGE_JSON = `{
  "name": "@xmtp/test-package",
  "version": "1.9.0",
  "description": "Test package",
  "main": "dist/index.js"
}
`;

describe("package.json manifest", () => {
  let tmpDir: string;
  let packageJsonPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
    packageJsonPath = path.join(tmpDir, "package.json");
    fs.writeFileSync(packageJsonPath, SAMPLE_PACKAGE_JSON);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("reads the version from package.json", () => {
    expect(readPackageJsonVersion(packageJsonPath)).toBe("1.9.0");
  });

  it("writes a new version to package.json", () => {
    writePackageJsonVersion(packageJsonPath, "2.0.0");
    expect(readPackageJsonVersion(packageJsonPath)).toBe("2.0.0");
  });

  it("writes dev and rc version suffixes", () => {
    writePackageJsonVersion(packageJsonPath, "1.9.1-dev.abc1234");
    expect(readPackageJsonVersion(packageJsonPath)).toBe("1.9.1-dev.abc1234");

    writePackageJsonVersion(packageJsonPath, "1.10.0-rc1");
    expect(readPackageJsonVersion(packageJsonPath)).toBe("1.10.0-rc1");
  });

  it("preserves other fields when writing", () => {
    writePackageJsonVersion(packageJsonPath, "5.0.0");
    const content = fs.readFileSync(packageJsonPath, "utf-8");
    const parsed = JSON.parse(content);
    expect(parsed.name).toBe("@xmtp/test-package");
    expect(parsed.version).toBe("5.0.0");
    expect(parsed.description).toBe("Test package");
    expect(parsed.main).toBe("dist/index.js");
  });

  it("preserves 2-space indentation", () => {
    writePackageJsonVersion(packageJsonPath, "2.0.0");
    const content = fs.readFileSync(packageJsonPath, "utf-8");
    expect(content).toContain('  "name"');
    expect(content).toContain('  "version"');
    expect(content.endsWith("\n")).toBe(true);
  });

  it("throws if version field is missing", () => {
    fs.writeFileSync(
      packageJsonPath,
      '{"name": "test"}\n',
    );
    expect(() => readPackageJsonVersion(packageJsonPath)).toThrow(
      "Could not find version",
    );
  });

  it("throws for non-existent file", () => {
    expect(() => readPackageJsonVersion("/nonexistent/package.json")).toThrow();
  });
});
