import { execSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, it, expect, beforeEach, afterEach } from "vitest";

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

    // Create JS SDK structures
    fs.mkdirSync(path.join(tmpDir, "sdks/node"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/node/package.json"),
      `{\n  "name": "@xmtp/node-sdk",\n  "version": "6.0.0"\n}\n`,
    );
    fs.mkdirSync(path.join(tmpDir, "sdks/browser"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/browser/package.json"),
      `{\n  "name": "@xmtp/browser-sdk",\n  "version": "7.0.0"\n}\n`,
    );
    fs.mkdirSync(path.join(tmpDir, "apps/cli"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "apps/cli/package.json"),
      `{\n  "name": "@xmtp/cli",\n  "version": "0.3.0"\n}\n`,
    );

    fs.mkdirSync(path.join(tmpDir, "sdks/agent"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/agent/package.json"),
      '{"name":"@xmtp/agent-sdk","version":"8.0.0"}\n',
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

    // The branch version does not change the Rust workspace version.
    const cargoToml = fs.readFileSync(path.join(tmpDir, "Cargo.toml"), "utf-8");
    expect(cargoToml).toContain('version = "0.0.0"');

    // Check release notes were created for iOS only
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/ios/1.0.1.md")),
    ).toBe(true);
    expect(fs.existsSync(path.join(tmpDir, "docs/release-notes/android"))).toBe(
      false,
    );

    // No matching git tag exists, so previous_release_tag should be omitted
    // but previous_release_version should be the pre-bump version
    const iosNotes = fs.readFileSync(
      path.join(tmpDir, "docs/release-notes/ios/1.0.1.md"),
      "utf-8",
    );
    expect(iosNotes).toContain('previous_release_version = "1.0.0"');
    expect(iosNotes).not.toContain("previous_release_tag");

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

  it("creates branch with node-sdk and browser-sdk bumps", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      nodeSdk: "minor",
      browserSdk: "patch",
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/1.1.0");

    // Check JS SDK versions were bumped off their own bases
    const nodeSdkPackageJson = JSON.parse(
      fs.readFileSync(path.join(tmpDir, "sdks/node/package.json"), "utf-8"),
    );
    expect(nodeSdkPackageJson.version).toBe("6.1.0");

    const browserSdkPackageJson = JSON.parse(
      fs.readFileSync(path.join(tmpDir, "sdks/browser/package.json"), "utf-8"),
    );
    expect(browserSdkPackageJson.version).toBe("7.0.1");

    // Check release notes were created for both JS SDKs
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/node-sdk/6.1.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/browser-sdk/7.0.1.md"),
      ),
    ).toBe(true);

    // Check commit message
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe(
      "chore: create release 1.1.0 (node-sdk 6.1.0, browser-sdk 7.0.1)",
    );
  });

  it("creates branch with a CLI bump", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      nodeSdk: "none",
      browserSdk: "none",
      agentSdk: "none",
      cli: "minor",
      $0: "test",
      _: [],
    });

    const cliPackageJson = JSON.parse(
      fs.readFileSync(path.join(tmpDir, "apps/cli/package.json"), "utf-8"),
    );
    expect(cliPackageJson.version).toBe("0.4.0");
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/cli/0.4.0.md")),
    ).toBe(true);
  });

  it("creates branch with all SDKs", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "2.0.0",
      base: "HEAD",
      ios: "major",
      android: "minor",
      nodeSdk: "major",
      browserSdk: "minor",
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/2.0.0");

    // Check all versions were set
    const podspec = fs.readFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      "utf-8",
    );
    expect(podspec).toContain('spec.version      = "2.0.0"');

    const gradle = fs.readFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      "utf-8",
    );
    expect(gradle).toContain("version=1.1.0");

    const nodeSdkPackageJson = JSON.parse(
      fs.readFileSync(path.join(tmpDir, "sdks/node/package.json"), "utf-8"),
    );
    expect(nodeSdkPackageJson.version).toBe("7.0.0");

    const browserSdkPackageJson = JSON.parse(
      fs.readFileSync(path.join(tmpDir, "sdks/browser/package.json"), "utf-8"),
    );
    expect(browserSdkPackageJson.version).toBe("7.1.0");

    // Check release notes were created for all
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/ios/2.0.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/android/1.1.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/node-sdk/7.0.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/browser-sdk/7.1.0.md"),
      ),
    ).toBe(true);

    // Check commit message includes all SDKs
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe(
      "chore: create release 2.0.0 (ios 2.0.0, android 1.1.0, node-sdk 7.0.0, browser-sdk 7.1.0)",
    );
  });

  it("keeps selected SDK versions and preserves prepared notes", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");
    const { getSdkConfig } = await import("../../src/lib/sdk-config");
    const sdks = ["ios", "android", "node-sdk", "browser-sdk", "agent-sdk"];
    for (const sdk of sdks) {
      getSdkConfig(sdk).manifest.writeVersion(tmpDir, "8.0.0");
    }
    const notesPath = path.join(tmpDir, "docs/release-notes/ios/8.0.0.md");
    fs.mkdirSync(path.dirname(notesPath), { recursive: true });
    const notes = "# iOS SDK 8.0.0\n\nReviewed migration instructions.\n";
    fs.writeFileSync(notesPath, notes);
    for (const sdk of sdks) {
      const preparedPath = path.join(
        tmpDir,
        "docs/release-notes",
        sdk,
        "8.0.0.md",
      );
      fs.mkdirSync(path.dirname(preparedPath), { recursive: true });
      fs.writeFileSync(preparedPath, notes);
    }
    const cargoPath = path.join(tmpDir, "Cargo.toml");
    fs.writeFileSync(
      cargoPath,
      '[workspace.package]\nversion = "1.12.0-dev"\n',
    );
    execSync("git add . && git commit -m 'prepare 8.0'", { cwd: tmpDir });
    const before = fs.readFileSync(cargoPath, "utf-8");
    const sourceCommit = execSync("git rev-parse HEAD", { cwd: tmpDir })
      .toString()
      .trim();

    handler({
      repoRoot: tmpDir,
      version: "8.0.0",
      base: "HEAD",
      ios: "keep",
      android: "keep",
      nodeSdk: "keep",
      browserSdk: "keep",
      agentSdk: "keep",
      _: [],
      $0: "test",
    });

    expect(
      execSync("git branch --show-current", { cwd: tmpDir }).toString().trim(),
    ).toBe("release/8.0.0");
    for (const sdk of sdks) {
      expect(getSdkConfig(sdk).manifest.readVersion(tmpDir)).toBe("8.0.0");
      expect(
        fs.existsSync(path.join(tmpDir, "docs/release-notes", sdk, "8.0.0.md")),
      ).toBe(true);
    }
    expect(fs.readFileSync(cargoPath, "utf-8")).toBe(before);
    expect(fs.readFileSync(notesPath, "utf-8")).toBe(notes);
    expect(
      JSON.parse(
        fs.readFileSync(
          path.join(tmpDir, "docs/release-notes/release-8.0.0.json"),
          "utf-8",
        ),
      ),
    ).toEqual({
      version: "8.0.0",
      sourceCommit,
      libxmtpVersion: "1.12.0-dev",
      sdks: Object.fromEntries(sdks.map((sdk) => [sdk, "8.0.0"])),
    });
    expect(
      execSync("git diff --name-only HEAD^ HEAD", { cwd: tmpDir })
        .toString()
        .trim(),
    ).toBe("docs/release-notes/release-8.0.0.json");
    expect(execSync("git status --porcelain", { cwd: tmpDir }).toString()).toBe(
      "",
    );
  });

  it("uses the last stable SDK tag for a kept version", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");
    execSync("git -c tag.gpgSign=false tag node-sdk-6.1.0", { cwd: tmpDir });
    execSync("git -c tag.gpgSign=false tag node-sdk-8.0.0-dev.abc1234", {
      cwd: tmpDir,
    });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/node/package.json"),
      '{"name":"@xmtp/node-sdk","version":"8.0.0"}\n',
    );
    execSync("git add . && git commit -m 'prepare Node 8'", { cwd: tmpDir });
    handler({
      repoRoot: tmpDir,
      version: "8.0.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      nodeSdk: "keep",
      _: [],
      $0: "test",
    });
    const notes = fs.readFileSync(
      path.join(tmpDir, "docs/release-notes/node-sdk/8.0.0.md"),
      "utf-8",
    );
    expect(notes).toContain('previous_release_version = "6.1.0"');
    expect(notes).toContain('previous_release_tag = "node-sdk-6.1.0"');
    expect(notes).not.toContain("dev.abc1234");
  });

  it("uses the repository root as the first-release note baseline", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");
    const root = execSync("git rev-parse HEAD", { cwd: tmpDir })
      .toString()
      .trim();
    handler({
      repoRoot: tmpDir,
      version: "8.0.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      agentSdk: "keep",
      _: [],
      $0: "test",
    });
    const notes = fs.readFileSync(
      path.join(tmpDir, "docs/release-notes/agent-sdk/8.0.0.md"),
      "utf-8",
    );
    expect(notes).toContain(`previous_release_tag = "${root}"`);
    expect(notes).not.toContain("previous_release_version");
  });

  it("sets the Rust workspace version only with an explicit option", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");
    handler({
      repoRoot: tmpDir,
      version: "8.0.0",
      base: "HEAD",
      libxmtpVersion: "1.12.0",
      ios: "none",
      android: "none",
      agentSdk: "keep",
      _: [],
      $0: "test",
    });
    expect(fs.readFileSync(path.join(tmpDir, "Cargo.toml"), "utf-8")).toContain(
      'version = "1.12.0"',
    );
  });

  it("throws error when no SDKs are selected", async () => {
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
    ).toThrow("Select at least one SDK");
  });

  it("includes previous_release_tag when matching git tag exists", async () => {
    // Create a git tag matching the current iOS manifest version
    execSync("git -c tag.gpgSign=false tag ios-1.0.0", { cwd: tmpDir });

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

    const iosNotes = fs.readFileSync(
      path.join(tmpDir, "docs/release-notes/ios/1.0.1.md"),
      "utf-8",
    );
    expect(iosNotes).toContain('previous_release_version = "1.0.0"');
    expect(iosNotes).toContain('previous_release_tag = "ios-1.0.0"');
  });
});
