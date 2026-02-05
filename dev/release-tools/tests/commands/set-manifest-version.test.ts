import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { setManifestVersion } from "../../src/commands/set-manifest-version.js";
import { readPodspecVersion } from "../../src/lib/manifest.js";

const SAMPLE_PODSPEC = `Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0"
end
`;

describe("setManifestVersion", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-set-version-"),
    );
    const podspecDir = path.join(tmpDir, "sdks", "ios");
    fs.mkdirSync(podspecDir, { recursive: true });
    fs.writeFileSync(path.join(podspecDir, "XMTP.podspec"), SAMPLE_PODSPEC);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("sets a release version", () => {
    setManifestVersion("ios", "5.0.0", tmpDir);
    expect(readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))).toBe(
      "5.0.0",
    );
  });

  it("sets a dev prerelease version", () => {
    setManifestVersion("ios", "4.10.0-dev.abc1234", tmpDir);
    expect(readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))).toBe(
      "4.10.0-dev.abc1234",
    );
  });

  it("sets an rc prerelease version", () => {
    setManifestVersion("ios", "4.10.0-rc.1", tmpDir);
    expect(readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))).toBe(
      "4.10.0-rc.1",
    );
  });

  it("returns the version that was set", () => {
    const result = setManifestVersion("ios", "5.0.0", tmpDir);
    expect(result).toBe("5.0.0");
  });

  it("throws for an unknown SDK", () => {
    expect(() => setManifestVersion("unknown", "1.0.0", tmpDir)).toThrow(
      "Unknown SDK: unknown",
    );
  });
});
