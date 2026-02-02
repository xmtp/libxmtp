import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { bumpVersion } from "../../src/commands/bump-version.js";
import { readPodspecVersion } from "../../src/lib/manifest.js";

const SAMPLE_PODSPEC = `Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0"
end
`;

describe("bumpVersion", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-bump-")
    );
    const podspecDir = path.join(tmpDir, "sdks", "ios");
    fs.mkdirSync(podspecDir, { recursive: true });
    fs.writeFileSync(
      path.join(podspecDir, "XMTP.podspec"),
      SAMPLE_PODSPEC
    );
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("bumps patch version", () => {
    const result = bumpVersion("ios", "patch", tmpDir);
    expect(result).toBe("4.9.1");
    expect(
      readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))
    ).toBe("4.9.1");
  });

  it("bumps minor version", () => {
    const result = bumpVersion("ios", "minor", tmpDir);
    expect(result).toBe("4.10.0");
  });

  it("bumps major version", () => {
    const result = bumpVersion("ios", "major", tmpDir);
    expect(result).toBe("5.0.0");
  });
});
