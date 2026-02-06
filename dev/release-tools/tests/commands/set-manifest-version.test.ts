import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { setManifestVersion } from "../../src/commands/set-manifest-version";
import {
  readPodspecVersion,
  readGradlePropertiesVersion,
} from "../../src/lib/manifest";

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
    // Set up iOS
    const podspecDir = path.join(tmpDir, "sdks", "ios");
    fs.mkdirSync(podspecDir, { recursive: true });
    fs.writeFileSync(path.join(podspecDir, "XMTP.podspec"), SAMPLE_PODSPEC);
    // Set up Android
    const androidDir = path.join(tmpDir, "sdks", "android");
    fs.mkdirSync(androidDir, { recursive: true });
    fs.writeFileSync(
      path.join(androidDir, "gradle.properties"),
      "org.gradle.jvmargs=-Xmx2048m\nversion=1.0.0\n",
    );
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("sets versions for iOS with various formats", () => {
    setManifestVersion("ios", "5.0.0", tmpDir);
    expect(readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))).toBe(
      "5.0.0",
    );

    setManifestVersion("ios", "4.10.0-dev.abc1234", tmpDir);
    expect(readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))).toBe(
      "4.10.0-dev.abc1234",
    );
  });

  it("sets versions for Android with various formats and preserves properties", () => {
    setManifestVersion("android", "2.0.0", tmpDir);
    expect(
      readGradlePropertiesVersion(
        path.join(tmpDir, "sdks/android/gradle.properties"),
      ),
    ).toBe("2.0.0");

    setManifestVersion("android", "2.0.0-dev.abc1234", tmpDir);
    expect(
      readGradlePropertiesVersion(
        path.join(tmpDir, "sdks/android/gradle.properties"),
      ),
    ).toBe("2.0.0-dev.abc1234");

    // Verify other properties preserved
    const content = fs.readFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      "utf-8",
    );
    expect(content).toContain("org.gradle.jvmargs=-Xmx2048m");
  });

  it("returns the version that was set", () => {
    expect(setManifestVersion("ios", "5.0.0", tmpDir)).toBe("5.0.0");
    expect(setManifestVersion("android", "2.0.0", tmpDir)).toBe("2.0.0");
  });

  it("throws for an unknown SDK", () => {
    expect(() => setManifestVersion("unknown", "1.0.0", tmpDir)).toThrow(
      "Unknown SDK: unknown",
    );
  });
});
