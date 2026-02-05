import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import {
  readPodspecVersion,
  writePodspecVersion,
  readGradlePropertiesVersion,
  writeGradlePropertiesVersion,
} from "../src/lib/manifest.js";

const SAMPLE_PODSPEC = `Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0"

  spec.summary      = "XMTP SDK Cocoapod"
end
`;

describe("podspec manifest", () => {
  let tmpDir: string;
  let podspecPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
    podspecPath = path.join(tmpDir, "XMTP.podspec");
    fs.writeFileSync(podspecPath, SAMPLE_PODSPEC);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("reads the version from a podspec", () => {
    expect(readPodspecVersion(podspecPath)).toBe("4.9.0");
  });

  it("writes a new version to a podspec", () => {
    writePodspecVersion(podspecPath, "4.10.0");
    expect(readPodspecVersion(podspecPath)).toBe("4.10.0");
  });

  it("preserves other content when writing", () => {
    writePodspecVersion(podspecPath, "5.0.0");
    const content = fs.readFileSync(podspecPath, "utf-8");
    expect(content).toContain('spec.name         = "XMTP"');
    expect(content).toContain('spec.version      = "5.0.0"');
    expect(content).toContain('spec.summary      = "XMTP SDK Cocoapod"');
  });

  it("throws if version line is not found", () => {
    fs.writeFileSync(podspecPath, "no version here\n");
    expect(() => readPodspecVersion(podspecPath)).toThrow();
  });
});

const SAMPLE_GRADLE_PROPERTIES = `# Project-wide Gradle settings.
org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
version=1.2.3
android.useAndroidX=true
`;

describe("gradle properties manifest", () => {
  let tmpDir: string;
  let propsPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
    propsPath = path.join(tmpDir, "gradle.properties");
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("reads and writes versions with various formats", () => {
    // Basic version
    fs.writeFileSync(propsPath, SAMPLE_GRADLE_PROPERTIES);
    expect(readGradlePropertiesVersion(propsPath)).toBe("1.2.3");

    // Dev version suffix
    writeGradlePropertiesVersion(propsPath, "1.2.3-dev.abc1234");
    expect(readGradlePropertiesVersion(propsPath)).toBe("1.2.3-dev.abc1234");

    // RC version suffix
    writeGradlePropertiesVersion(propsPath, "1.2.3-rc1");
    expect(readGradlePropertiesVersion(propsPath)).toBe("1.2.3-rc1");
  });

  it("reads version at different positions in file", () => {
    // First line
    fs.writeFileSync(propsPath, "version=2.0.0\nother=value\n");
    expect(readGradlePropertiesVersion(propsPath)).toBe("2.0.0");

    // Last line without trailing newline
    fs.writeFileSync(propsPath, "other=value\nversion=3.0.0");
    expect(readGradlePropertiesVersion(propsPath)).toBe("3.0.0");

    // With whitespace around equals sign
    fs.writeFileSync(propsPath, "version = 4.0.0\n");
    expect(readGradlePropertiesVersion(propsPath)).toBe("4.0.0");
  });

  it("preserves other content and comments when writing", () => {
    fs.writeFileSync(propsPath, SAMPLE_GRADLE_PROPERTIES);
    writeGradlePropertiesVersion(propsPath, "5.0.0");
    const content = fs.readFileSync(propsPath, "utf-8");
    expect(content).toContain("# Project-wide Gradle settings.");
    expect(content).toContain("org.gradle.jvmargs=-Xmx2048m");
    expect(content).toContain("android.useAndroidX=true");
    expect(content).toContain("version=5.0.0");
    expect(content).not.toContain("version=1.2.3");
  });

  it("appends version if not present", () => {
    fs.writeFileSync(propsPath, "other=value\n");
    writeGradlePropertiesVersion(propsPath, "1.0.0");
    expect(readGradlePropertiesVersion(propsPath)).toBe("1.0.0");
  });

  it("throws for missing or invalid version lines", () => {
    // No version line
    fs.writeFileSync(propsPath, "other=value\n");
    expect(() => readGradlePropertiesVersion(propsPath)).toThrow(
      "Could not find version=",
    );

    // Commented version line
    fs.writeFileSync(propsPath, "# version=1.0.0\n");
    expect(() => readGradlePropertiesVersion(propsPath)).toThrow(
      "Could not find version=",
    );

    // Version as part of property name
    fs.writeFileSync(propsPath, "myversion=1.0.0\n");
    expect(() => readGradlePropertiesVersion(propsPath)).toThrow(
      "Could not find version=",
    );

    // Non-existent file
    expect(() => readGradlePropertiesVersion("/nonexistent")).toThrow();
  });
});
