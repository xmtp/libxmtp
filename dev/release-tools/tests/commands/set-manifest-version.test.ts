import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { setManifestVersion } from "../../src/commands/set-manifest-version";

describe("application-only version setter", () => {
  let root: string;
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "cli-version-"));
    fs.mkdirSync(path.join(root, "apps/cli"), { recursive: true });
    fs.writeFileSync(
      path.join(root, "apps/cli/package.json"),
      '{"version":"1.0.0"}',
    );
  });
  afterEach(() => fs.rmSync(root, { recursive: true, force: true }));
  it("keeps CLI versioning independent", () => {
    expect(setManifestVersion("cli", "1.1.0-dev.fixture", root)).toBe(
      "1.1.0-dev.fixture",
    );
    expect(
      JSON.parse(
        fs.readFileSync(path.join(root, "apps/cli/package.json"), "utf8"),
      ).version,
    ).toBe("1.1.0-dev.fixture");
  });
  it.each([
    "ios",
    "android",
    "node-sdk",
    "browser-sdk",
    "agent-sdk",
    "react-native-sdk",
    "libxmtp",
  ])("rejects independent %s mutation", (sdk) => {
    expect(() => setManifestVersion(sdk, "99.0.0", root)).toThrow(
      "SDK versions are shared",
    );
  });
});
