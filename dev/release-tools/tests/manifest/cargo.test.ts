import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { readCargoVersion, writeCargoVersion } from "../../src/lib/manifest";

const SAMPLE_CARGO_TOML = `[workspace]
members = [
  "crates/*",
]
resolver = "3"

[workspace.package]
license = "MIT"
version = "1.9.0"

[workspace.dependencies]
serde = "1.0"
tokio = { version = "1.47.0", default-features = false }
`;

describe("cargo manifest", () => {
  let tmpDir: string;
  let cargoTomlPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
    cargoTomlPath = path.join(tmpDir, "Cargo.toml");
    fs.writeFileSync(cargoTomlPath, SAMPLE_CARGO_TOML);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("reads the version from Cargo.toml", () => {
    expect(readCargoVersion(cargoTomlPath)).toBe("1.9.0");
  });

  it("writes a new version to Cargo.toml", () => {
    writeCargoVersion(cargoTomlPath, "2.0.0", tmpDir);
    expect(readCargoVersion(cargoTomlPath)).toBe("2.0.0");
  });

  it("writes dev and rc version suffixes", () => {
    writeCargoVersion(cargoTomlPath, "1.9.1-dev.abc1234", tmpDir);
    expect(readCargoVersion(cargoTomlPath)).toBe("1.9.1-dev.abc1234");

    writeCargoVersion(cargoTomlPath, "1.10.0-rc1", tmpDir);
    expect(readCargoVersion(cargoTomlPath)).toBe("1.10.0-rc1");
  });

  it("preserves comments and other content when writing", () => {
    writeCargoVersion(cargoTomlPath, "2.0.0", tmpDir);
    const content = fs.readFileSync(cargoTomlPath, "utf-8");
    expect(content).toContain("[workspace]");
    expect(content).toContain("members = [");
    expect(content).toContain('license = "MIT"');
    expect(content).toContain('version = "2.0.0"');
    expect(content).toContain('serde = "1.0"');
    expect(content).not.toContain('version = "1.9.0"');
  });

  it("throws if workspace.package section is missing", () => {
    fs.writeFileSync(
      cargoTomlPath,
      '[package]\nname = "foo"\nversion = "1.0.0"\n',
    );
    expect(() => readCargoVersion(cargoTomlPath)).toThrow(
      "Could not find workspace.package.version",
    );
  });

  it("throws if version is missing from workspace.package", () => {
    fs.writeFileSync(cargoTomlPath, '[workspace.package]\nlicense = "MIT"\n');
    expect(() => readCargoVersion(cargoTomlPath)).toThrow(
      "Could not find workspace.package.version",
    );
  });

  it("throws on write if workspace.package.version pattern not found", () => {
    fs.writeFileSync(cargoTomlPath, '[package]\nname = "foo"\n');
    expect(() => writeCargoVersion(cargoTomlPath, "1.0.0", tmpDir)).toThrow(
      "Could not find workspace.package.version",
    );
  });

  it("throws for non-existent file", () => {
    expect(() => readCargoVersion("/nonexistent/Cargo.toml")).toThrow();
  });
});
