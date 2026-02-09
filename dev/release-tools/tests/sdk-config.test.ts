import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { getSdkConfig, SDK_CONFIGS } from "../src/lib/sdk-config";
import { Sdk } from "../src/types";

describe("SDK configs", () => {
  it("returns correct config for each SDK", () => {
    const ios = getSdkConfig("ios");
    expect(ios.name).toBe("iOS");
    expect(ios.tagPrefix).toBe("ios-");

    const android = getSdkConfig("android");
    expect(android.name).toBe("Android");
    expect(android.tagPrefix).toBe("android-");

    const node = getSdkConfig("node");
    expect(node.name).toBe("Node");
    expect(node.tagPrefix).toBe("node-bindings-");

    const wasm = getSdkConfig("wasm");
    expect(wasm.name).toBe("WASM");
    expect(wasm.tagPrefix).toBe("wasm-bindings-");

    const libxmtp = getSdkConfig("libxmtp");
    expect(libxmtp.name).toBe("Libxmtp");
    expect(libxmtp.tagPrefix).toBe("v");
  });

  it("throws for unknown SDK with available options", () => {
    expect(() => getSdkConfig("unknown")).toThrow(
      "Unknown SDK: unknown. Available: ios, android, node, wasm, libxmtp",
    );
  });

  it("has config for all SDK enum values", () => {
    for (const sdk of Object.values(Sdk)) {
      expect(SDK_CONFIGS[sdk]).toBeDefined();
      expect(SDK_CONFIGS[sdk].manifest).toBeDefined();
    }
  });

  describe("Libxmtp manifest provider", () => {
    let tmpDir: string;

    beforeEach(() => {
      tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
      fs.writeFileSync(
        path.join(tmpDir, "Cargo.toml"),
        '[workspace.package]\nversion = "1.9.0"\n',
      );
    });

    afterEach(() => {
      fs.rmSync(tmpDir, { recursive: true });
    });

    it("reads and writes version via manifest provider", () => {
      const config = getSdkConfig("libxmtp");
      expect(config.manifest.readVersion(tmpDir)).toBe("1.9.0");

      config.manifest.writeVersion(tmpDir, "2.0.0-dev.abc1234");
      expect(config.manifest.readVersion(tmpDir)).toBe("2.0.0-dev.abc1234");
    });
  });

  describe("Node manifest provider", () => {
    let tmpDir: string;

    beforeEach(() => {
      tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
      fs.mkdirSync(path.join(tmpDir, "bindings", "node"), { recursive: true });
      fs.writeFileSync(
        path.join(tmpDir, "bindings", "node", "package.json"),
        '{\n  "name": "@xmtp/node-bindings",\n  "version": "1.10.0"\n}\n',
      );
    });

    afterEach(() => {
      fs.rmSync(tmpDir, { recursive: true });
    });

    it("reads and writes version via manifest provider", () => {
      const config = getSdkConfig("node");
      expect(config.manifest.readVersion(tmpDir)).toBe("1.10.0");

      config.manifest.writeVersion(tmpDir, "1.11.0-dev.abc1234");
      expect(config.manifest.readVersion(tmpDir)).toBe("1.11.0-dev.abc1234");
    });
  });

  describe("WASM manifest provider", () => {
    let tmpDir: string;

    beforeEach(() => {
      tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
      fs.mkdirSync(path.join(tmpDir, "bindings", "wasm"), { recursive: true });
      fs.writeFileSync(
        path.join(tmpDir, "bindings", "wasm", "package.json"),
        '{\n  "name": "@xmtp/wasm-bindings",\n  "version": "1.10.0"\n}\n',
      );
    });

    afterEach(() => {
      fs.rmSync(tmpDir, { recursive: true });
    });

    it("reads and writes version via manifest provider", () => {
      const config = getSdkConfig("wasm");
      expect(config.manifest.readVersion(tmpDir)).toBe("1.10.0");

      config.manifest.writeVersion(tmpDir, "1.11.0-dev.abc1234");
      expect(config.manifest.readVersion(tmpDir)).toBe("1.11.0-dev.abc1234");
    });
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
