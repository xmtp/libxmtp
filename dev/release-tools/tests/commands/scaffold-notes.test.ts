import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { scaffoldNotes } from "../../src/commands/scaffold-notes.js";

function gitTag(name: string, cwd: string) {
  execSync(`git -c tag.gpgSign=false tag ${name}`, { cwd });
}

describe("scaffoldNotes", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-notes-")
    );
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    execSync("git config commit.gpgSign false", { cwd: tmpDir });
    // Create directory structure
    fs.mkdirSync(path.join(tmpDir, "sdks/ios"), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, "docs/release-notes"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      `Pod::Spec.new do |spec|\n  spec.version      = "4.10.0"\nend\n`
    );
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial commit'", {
      cwd: tmpDir,
    });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("creates release notes from a since tag", () => {
    gitTag("ios-4.9.0", tmpDir);
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "change1");
    execSync("git add . && git commit -m 'feat: new feature'", {
      cwd: tmpDir,
    });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "change2");
    execSync("git add . && git commit -m 'fix: bug fix'", {
      cwd: tmpDir,
    });

    const outputPath = scaffoldNotes("ios", tmpDir, "ios-4.9.0");
    expect(outputPath).toContain("docs/release-notes/ios-4.10.0.md");

    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain("new feature");
    expect(content).toContain("bug fix");
  });

  it("handles no previous tag gracefully", () => {
    const outputPath = scaffoldNotes("ios", tmpDir, null);
    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain("first release from the monorepo");
  });
});
