import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { listTags, getShortSha, getCommitsBetween } from "../src/lib/git";

function gitTag(name: string, cwd: string) {
  execSync(`git -c tag.gpgSign=false tag ${name}`, { cwd });
}

describe("git helpers", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-git-"));
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    // Disable commit signing in the test repo
    execSync("git config commit.gpgSign false", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial commit'", { cwd: tmpDir });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  describe("listTags", () => {
    it("returns all tags", () => {
      gitTag("ios-4.8.0", tmpDir);
      gitTag("ios-4.9.0", tmpDir);
      gitTag("android-1.0.0", tmpDir);
      const tags = listTags(tmpDir);
      expect(tags).toContain("ios-4.8.0");
      expect(tags).toContain("ios-4.9.0");
      expect(tags).toContain("android-1.0.0");
    });

    it("returns empty array for repo with no tags", () => {
      expect(listTags(tmpDir)).toEqual([]);
    });
  });

  describe("getShortSha", () => {
    it("returns a 7-character hash", () => {
      const sha = getShortSha(tmpDir);
      expect(sha).toMatch(/^[0-9a-f]{7}$/);
    });
  });

  describe("getCommitsBetween", () => {
    it("returns commits between two refs", () => {
      gitTag("v1", tmpDir);
      fs.writeFileSync(path.join(tmpDir, "file.txt"), "change1");
      execSync("git add . && git commit -m 'feat: add feature'", {
        cwd: tmpDir,
      });
      fs.writeFileSync(path.join(tmpDir, "file.txt"), "change2");
      execSync("git add . && git commit -m 'fix: bug fix'", {
        cwd: tmpDir,
      });
      const commits = getCommitsBetween(tmpDir, "v1", "HEAD");
      expect(commits).toHaveLength(2);
      expect(commits[0]).toContain("fix: bug fix");
      expect(commits[1]).toContain("feat: add feature");
    });

    it("returns all commits from HEAD when sinceRef is null", () => {
      const commits = getCommitsBetween(tmpDir, null, "HEAD");
      expect(commits.length).toBeGreaterThan(0);
    });
  });
});
