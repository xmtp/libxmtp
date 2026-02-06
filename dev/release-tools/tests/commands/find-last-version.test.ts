import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { findLastVersion } from "../../src/commands/find-last-version";

function gitTag(name: string, cwd: string) {
  execSync(`git -c tag.gpgSign=false tag ${name}`, { cwd });
}

describe("findLastVersion", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-flv-"));
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    execSync("git config commit.gpgSign false", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial'", { cwd: tmpDir });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("finds the latest stable ios version", () => {
    gitTag("ios-4.8.0", tmpDir);
    gitTag("ios-4.9.0", tmpDir);
    gitTag("ios-4.9.0-libxmtp", tmpDir);
    expect(findLastVersion("ios", tmpDir)).toBe("4.9.0");
  });

  it("returns null when no tags exist", () => {
    expect(findLastVersion("ios", tmpDir)).toBeNull();
  });

  it("skips prerelease by default", () => {
    gitTag("ios-4.9.0", tmpDir);
    gitTag("ios-4.10.0-rc1", tmpDir);
    expect(findLastVersion("ios", tmpDir)).toBe("4.9.0");
  });

  it("includes prerelease when requested", () => {
    gitTag("ios-4.9.0", tmpDir);
    gitTag("ios-4.10.0-rc1", tmpDir);
    expect(findLastVersion("ios", tmpDir, true)).toBe("4.10.0-rc1");
  });
});
