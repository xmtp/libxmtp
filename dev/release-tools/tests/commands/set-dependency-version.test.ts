import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { setPackageJsonDependency } from "../../src/lib/manifest";

const SAMPLE_PACKAGE_JSON = `{
  "name": "@xmtp/node-sdk",
  "version": "6.0.0",
  "description": "XMTP Node client SDK",
  "dependencies": {
    "@xmtp/content-type-primitives": "3.0.0",
    "@xmtp/node-bindings": "portal:../../../bindings/node"
  }
}
`;

describe("setPackageJsonDependency", () => {
  let tmpDir: string;
  let packageJsonPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-set-dep-"),
    );
    packageJsonPath = path.join(tmpDir, "package.json");
    fs.writeFileSync(packageJsonPath, SAMPLE_PACKAGE_JSON);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("rewrites a portal: spec to a real published version", () => {
    setPackageJsonDependency(
      packageJsonPath,
      "@xmtp/node-bindings",
      "1.11.0-nightly.20260604.90d0bfb",
    );
    const parsed = JSON.parse(fs.readFileSync(packageJsonPath, "utf-8"));
    expect(parsed.dependencies["@xmtp/node-bindings"]).toBe(
      "1.11.0-nightly.20260604.90d0bfb",
    );
  });

  it("leaves all other fields and formatting intact", () => {
    setPackageJsonDependency(
      packageJsonPath,
      "@xmtp/node-bindings",
      "1.11.0",
    );
    const content = fs.readFileSync(packageJsonPath, "utf-8");
    const parsed = JSON.parse(content);

    // Other top-level fields preserved
    expect(parsed.name).toBe("@xmtp/node-sdk");
    expect(parsed.version).toBe("6.0.0");
    expect(parsed.description).toBe("XMTP Node client SDK");

    // Sibling dependency preserved
    expect(parsed.dependencies["@xmtp/content-type-primitives"]).toBe("3.0.0");

    // 2-space indentation preserved
    expect(content).toContain('  "name"');
    expect(content).toContain('  "dependencies"');
  });

  it("can rewrite a semver dep to a nightly version", () => {
    setPackageJsonDependency(
      packageJsonPath,
      "@xmtp/content-type-primitives",
      "4.0.0-nightly.20260604.abc1234",
    );
    const parsed = JSON.parse(fs.readFileSync(packageJsonPath, "utf-8"));
    expect(parsed.dependencies["@xmtp/content-type-primitives"]).toBe(
      "4.0.0-nightly.20260604.abc1234",
    );
  });

  it("throws when the dependency does not exist in dependencies", () => {
    expect(() =>
      setPackageJsonDependency(
        packageJsonPath,
        "@xmtp/nonexistent-package",
        "1.0.0",
      ),
    ).toThrow(
      "Dependency @xmtp/nonexistent-package not found in dependencies of",
    );
  });

  it("throws when the package.json has no dependencies block", () => {
    fs.writeFileSync(
      packageJsonPath,
      '{ "name": "minimal", "version": "1.0.0" }\n',
    );
    expect(() =>
      setPackageJsonDependency(packageJsonPath, "@xmtp/node-bindings", "1.0.0"),
    ).toThrow("Dependency @xmtp/node-bindings not found in dependencies of");
  });
});
