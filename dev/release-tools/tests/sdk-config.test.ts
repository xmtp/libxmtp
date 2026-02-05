import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { getSdkConfig, SDK_CONFIGS } from "../src/lib/sdk-config.js";
import { Sdk } from "../src/types.js";

describe("SDK configs", () => {
  it("returns correct config for each SDK", () => {
    const ios = getSdkConfig("ios");
    expect(ios.name).toBe("iOS");
    expect(ios.tagPrefix).toBe("ios-");

    const android = getSdkConfig("android");
    expect(android.name).toBe("Android");
    expect(android.tagPrefix).toBe("android-");
  });

  it("throws for unknown SDK with available options", () => {
    expect(() => getSdkConfig("unknown")).toThrow(
      "Unknown SDK: unknown. Available: ios, android",
    );
  });

  it("has config for all SDK enum values", () => {
    for (const sdk of Object.values(Sdk)) {
      expect(SDK_CONFIGS[sdk]).toBeDefined();
      expect(SDK_CONFIGS[sdk].manifest).toBeDefined();
    }
  });

  describe("Android manifest provider", () => {
    let tmpDir: string;

    beforeEach(() => {
      tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
      fs.mkdirSync(path.join(tmpDir, "sdks", "android"), { recursive: true });
      fs.writeFileSync(
        path.join(tmpDir, "sdks", "android", "gradle.properties"),
        "version=4.9.0\n",
      );
    });

    afterEach(() => {
      fs.rmSync(tmpDir, { recursive: true });
    });

    it("reads and writes version via manifest provider", () => {
      const config = getSdkConfig("android");
      expect(config.manifest.readVersion(tmpDir)).toBe("4.9.0");

      config.manifest.writeVersion(tmpDir, "4.10.0-dev.abc1234");
      expect(config.manifest.readVersion(tmpDir)).toBe("4.10.0-dev.abc1234");
    });
  });
});
