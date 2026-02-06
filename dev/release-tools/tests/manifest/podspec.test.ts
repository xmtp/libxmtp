import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import {
  readPodspecVersion,
  writePodspecVersion,
} from "../../src/lib/manifest";

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
