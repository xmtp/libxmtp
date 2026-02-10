import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

// We test the handler indirectly by importing and calling with mock argv
// Since the handler uses execSync for git commands, we set up a real git repo

describe("create-release-branch", () => {
  let tmpDir: string;

  function setupTestRepo() {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-create-branch-"),
    );
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    execSync("git config commit.gpgSign false", { cwd: tmpDir });

    // Create iOS SDK structure
    fs.mkdirSync(path.join(tmpDir, "sdks/ios"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      `Pod::Spec.new do |spec|\n  spec.version      = "1.0.0"\nend\n`,
    );

    // Create Android SDK structure
    fs.mkdirSync(path.join(tmpDir, "sdks/android"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      `version=1.0.0\n`,
    );

    // Create libxmtp (Cargo.toml) structure
    fs.writeFileSync(
      path.join(tmpDir, "Cargo.toml"),
      `[workspace.package]\nversion = "0.0.0"\n`,
    );

    // Create release notes directory
    fs.mkdirSync(path.join(tmpDir, "docs/release-notes"), { recursive: true });

    // Initial commit
    execSync("git add . && git commit -m 'initial commit'", { cwd: tmpDir });

    return tmpDir;
  }

  beforeEach(() => {
    setupTestRepo();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("creates branch with single iOS bump", async () => {
    // Dynamically import to get fresh module
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "patch",
      android: "none",
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/1.1.0");

    // Check iOS version was bumped
    const podspec = fs.readFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      "utf-8",
    );
    expect(podspec).toContain('spec.version      = "1.0.1"');

    // Check Android version was NOT bumped
    const gradle = fs.readFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      "utf-8",
    );
    expect(gradle).toContain("version=1.0.0");

    // Check Cargo.toml version was set to --version
    const cargoToml = fs.readFileSync(path.join(tmpDir, "Cargo.toml"), "utf-8");
    expect(cargoToml).toContain('version = "1.1.0"');

    // Check release notes were created for iOS only
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/ios/1.0.1.md")),
    ).toBe(true);
    expect(fs.existsSync(path.join(tmpDir, "docs/release-notes/android"))).toBe(
      false,
    );

    // Check commit message
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe("chore: create release 1.1.0 (ios 1.0.1)");
  });

  it("creates branch with single Android bump", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "none",
      android: "minor",
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/1.1.0");

    // Check Android version was bumped
    const gradle = fs.readFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      "utf-8",
    );
    expect(gradle).toContain("version=1.1.0");

    // Check iOS version was NOT bumped
    const podspec = fs.readFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      "utf-8",
    );
    expect(podspec).toContain('spec.version      = "1.0.0"');

    // Check release notes were created for Android only
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/android/1.1.0.md")),
    ).toBe(true);

    // Check commit message
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe("chore: create release 1.1.0 (android 1.1.0)");
  });

  it("creates branch with both iOS and Android bumps", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "2.0.0",
      base: "HEAD",
      ios: "major",
      android: "minor",
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/2.0.0");

    // Check iOS version was bumped (major)
    const podspec = fs.readFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      "utf-8",
    );
    expect(podspec).toContain('spec.version      = "2.0.0"');

    // Check Android version was bumped (minor)
    const gradle = fs.readFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      "utf-8",
    );
    expect(gradle).toContain("version=1.1.0");

    // Check release notes were created for both
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/ios/2.0.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/android/1.1.0.md")),
    ).toBe(true);

    // Check commit message includes both SDKs
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe(
      "chore: create release 2.0.0 (ios 2.0.0, android 1.1.0)",
    );
  });

  it("throws error when no SDKs are bumped", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    expect(() =>
      handler({
        repoRoot: tmpDir,
        version: "1.1.0",
        base: "HEAD",
        ios: "none",
        android: "none",
        _: [],
        $0: "",
      }),
    ).toThrow("At least one SDK must be bumped");
  });
});
