import { execSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, it, expect, beforeEach, afterEach } from "vitest";

import {
  listTags,
  getShortSha,
  getCommitsBetween,
  createTag,
  pushTag,
} from "../src/lib/git";

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
    // Disable commit and tag signing in the test repo
    execSync("git config commit.gpgSign false", { cwd: tmpDir });
    execSync("git config tag.gpgSign false", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial commit'", { cwd: tmpDir });
    execSync("git branch -M main", { cwd: tmpDir });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  describe("listTags", () => {
    it("returns all tags", () => {
      expect(listTags(tmpDir)).toEqual([]);
      gitTag("ios-4.8.0", tmpDir);
      gitTag("ios-4.9.0", tmpDir);
      gitTag("android-1.0.0", tmpDir);
      const tags = listTags(tmpDir);
      expect(tags).toContain("ios-4.8.0");
      expect(tags).toContain("ios-4.9.0");
      expect(tags).toContain("android-1.0.0");
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
      const fromHead = getCommitsBetween(tmpDir, null, "HEAD");
      expect(fromHead).toHaveLength(3);
      expect(fromHead[0]).toContain("fix: bug fix");
      expect(fromHead[1]).toContain("feat: add feature");
      expect(fromHead[2]).toContain("initial commit");
    });
  });

  describe("createTag", () => {
    it("creates a tag", () => {
      createTag(tmpDir, "v1.0.0");
      const tags = listTags(tmpDir);
      expect(tags).toContain("v1.0.0");
      expect(() => createTag(tmpDir, "v1.0.0")).toThrow(
        "Tag v1.0.0 already exists",
      );
      createTag(tmpDir, "v1.0.0", true);
      expect(listTags(tmpDir)).toEqual(tags);
    });
  });

  describe("pushTag", () => {
    let remoteDir: string;

    beforeEach(() => {
      // Create a bare remote repo to push to
      remoteDir = fs.mkdtempSync(
        path.join(os.tmpdir(), "release-tools-remote-"),
      );
      execSync("git init --bare", { cwd: remoteDir });
      execSync(`git remote add origin ${remoteDir}`, { cwd: tmpDir });
      // Push initial commit so remote has a ref
      execSync("git push origin HEAD:main", { cwd: tmpDir });
    });

    afterEach(() => {
      fs.rmSync(remoteDir, { recursive: true });
    });

    it("pushes a tag to remote", () => {
      gitTag("v1.0.0", tmpDir);
      const firstTagSha = execSync("git rev-parse v1.0.0", {
        cwd: tmpDir,
        encoding: "utf-8",
      }).trim();
      pushTag(tmpDir, "v1.0.0", false);
      const remoteTagSha = execSync("git rev-parse refs/tags/v1.0.0", {
        cwd: remoteDir,
        encoding: "utf-8",
      }).trim();
      expect(remoteTagSha).toBe(firstTagSha);

      fs.writeFileSync(path.join(tmpDir, "file.txt"), "updated");
      execSync("git add . && git commit -m 'update'", { cwd: tmpDir });
      gitTag("v2.0.0", tmpDir);
      pushTag(tmpDir, "v2.0.0", true);
      const localHead = execSync("git rev-parse HEAD", {
        cwd: tmpDir,
        encoding: "utf-8",
      }).trim();
      const remoteHead = execSync("git rev-parse refs/heads/main", {
        cwd: remoteDir,
        encoding: "utf-8",
      }).trim();
      const secondTagSha = execSync("git rev-parse refs/tags/v2.0.0", {
        cwd: remoteDir,
        encoding: "utf-8",
      }).trim();
      expect(remoteHead).toBe(localHead);
      expect(secondTagSha).toBe(localHead);

      // A conflicting local tag must not replace an existing remote tag.
      execSync("git tag -f v1.0.0 HEAD", { cwd: tmpDir });
      pushTag(tmpDir, "v1.0.0", false, true);
      const retainedTagSha = execSync("git rev-parse refs/tags/v1.0.0", {
        cwd: remoteDir,
        encoding: "utf-8",
      }).trim();
      expect(retainedTagSha).toBe(firstTagSha);
    });
  });
});
