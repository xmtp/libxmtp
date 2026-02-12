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

    // Create Node bindings structure
    fs.mkdirSync(path.join(tmpDir, "bindings/node"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "bindings/node/package.json"),
      `{\n  "name": "@xmtp/node-bindings",\n  "version": "0.0.0"\n}\n`,
    );

    // Create WASM bindings structure
    fs.mkdirSync(path.join(tmpDir, "bindings/wasm"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "bindings/wasm/package.json"),
      `{\n  "name": "@xmtp/wasm-bindings",\n  "version": "0.0.0"\n}\n`,
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
      node: false,
      wasm: false,
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
      node: false,
      wasm: false,
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
      node: false,
      wasm: false,
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

  it("creates branch with --node flag", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      node: true,
      wasm: false,
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/1.1.0");

    // Check Node version was set to release version
    const packageJson = JSON.parse(
      fs.readFileSync(
        path.join(tmpDir, "bindings/node/package.json"),
        "utf-8",
      ),
    );
    expect(packageJson.version).toBe("1.1.0");

    // Check release notes were created for node
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/node-bindings/1.1.0.md"),
      ),
    ).toBe(true);

    // Check commit message
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe("chore: create release 1.1.0 (node-bindings 1.1.0)");
  });

  it("creates branch with --wasm flag", async () => {
    const { handler } =
      await import("../../src/commands/create-release-branch");

    handler({
      repoRoot: tmpDir,
      version: "1.1.0",
      base: "HEAD",
      ios: "none",
      android: "none",
      node: false,
      wasm: true,
      _: [],
      $0: "",
    });

    // Check branch was created
    const branch = execSync("git branch --show-current", { cwd: tmpDir })
      .toString()
      .trim();
    expect(branch).toBe("release/1.1.0");

    // Check WASM version was set to release version
    const packageJson = JSON.parse(
      fs.readFileSync(
        path.join(tmpDir, "bindings/wasm/package.json"),
        "utf-8",
      ),
    );
    expect(packageJson.version).toBe("1.1.0");

    // Check release notes were created for wasm
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/wasm-bindings/1.1.0.md"),
      ),
    ).toBe(true);

    // Check commit message
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe("chore: create release 1.1.0 (wasm-bindings 1.1.0)");
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
      node: true,
      wasm: true,
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

    const nodePackageJson = JSON.parse(
      fs.readFileSync(
        path.join(tmpDir, "bindings/node/package.json"),
        "utf-8",
      ),
    );
    expect(nodePackageJson.version).toBe("2.0.0");

    const wasmPackageJson = JSON.parse(
      fs.readFileSync(
        path.join(tmpDir, "bindings/wasm/package.json"),
        "utf-8",
      ),
    );
    expect(wasmPackageJson.version).toBe("2.0.0");

    // Check release notes were created for all
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/ios/2.0.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(path.join(tmpDir, "docs/release-notes/android/1.1.0.md")),
    ).toBe(true);
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/node-bindings/2.0.0.md"),
      ),
    ).toBe(true);
    expect(
      fs.existsSync(
        path.join(tmpDir, "docs/release-notes/wasm-bindings/2.0.0.md"),
      ),
    ).toBe(true);

    // Check commit message includes all SDKs
    const commitMsg = execSync("git log -1 --pretty=%B", { cwd: tmpDir })
      .toString()
      .trim();
    expect(commitMsg).toBe(
      "chore: create release 2.0.0 (ios 2.0.0, android 1.1.0, node-bindings 2.0.0, wasm-bindings 2.0.0)",
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
        node: false,
        wasm: false,
        _: [],
        $0: "",
      }),
    ).toThrow("At least one SDK must be bumped");
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
      node: false,
      wasm: false,
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
