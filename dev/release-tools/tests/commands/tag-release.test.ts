import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, it, expect } from "vitest";

import { buildTag } from "../../src/commands/tag-release";

describe("buildTag", () => {
  it("builds iOS tag with prefix", () => {
    expect(buildTag("ios", "4.9.0")).toBe("ios-4.9.0");
  });

  it("builds Android tag with prefix", () => {
    expect(buildTag("android", "1.2.3")).toBe("android-1.2.3");
  });

  it("handles dev versions", () => {
    expect(buildTag("ios", "4.9.0-dev.abc1234")).toBe("ios-4.9.0-dev.abc1234");
    expect(buildTag("android", "1.2.3-dev.abc1234")).toBe(
      "android-1.2.3-dev.abc1234",
    );
  });

  it("handles rc versions", () => {
    expect(buildTag("ios", "4.9.0-rc1")).toBe("ios-4.9.0-rc1");
    expect(buildTag("android", "1.2.3-rc2")).toBe("android-1.2.3-rc2");
  });

  it("throws for unknown SDK", () => {
    expect(() => buildTag("unknown", "1.0.0")).toThrow("Unknown SDK: unknown");
  });

  it("throws for empty version", () => {
    expect(() => buildTag("ios", "")).toThrow("Invalid version");
  });

  it("throws for non-semver version", () => {
    expect(() => buildTag("ios", "not-a-version")).toThrow("Invalid version");
    expect(() => buildTag("android", "1.2")).toThrow("Invalid version");
  });
});

describe("tag-release CLI", () => {
  it.each([false, true])("creates a tag with push disabled: %s", (noPush) => {
    const repoRoot = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-local-tag-"),
    );
    try {
      const git = (...args: string[]) =>
        execFileSync("git", args, { cwd: repoRoot, encoding: "utf-8" }).trim();
      git("init", "--initial-branch=main");
      git("config", "user.name", "Test");
      git("config", "user.email", "test@example.com");
      git("config", "tag.gpgSign", "false");
      git(
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--allow-empty",
        "-m",
        "source",
      );
      const remote = path.join(repoRoot, "remote");
      if (!noPush) {
        git("init", "--bare", remote);
      }
      git("remote", "add", "origin", remote);
      const output = execFileSync(
        path.resolve("node_modules/.bin/tsx"),
        [
          "--tsconfig",
          path.resolve("tsconfig.json"),
          path.resolve("src/cli.ts"),
          "tag-release",
          "--sdk",
          "ios",
          "--version",
          "1.2.3",
          "--repo-root",
          repoRoot,
          ...(noPush ? ["--no-push"] : []),
        ],
        { cwd: repoRoot, encoding: "utf-8" },
      );
      expect(output.trim()).toBe("ios-1.2.3");
      expect(git("rev-parse", "ios-1.2.3")).toBe(git("rev-parse", "HEAD"));
      if (noPush) {
        expect(fs.existsSync(remote)).toBe(false);
      } else {
        expect(git("--git-dir=" + remote, "rev-parse", "ios-1.2.3")).toBe(
          git("rev-parse", "HEAD"),
        );
      }
    } finally {
      fs.rmSync(repoRoot, { recursive: true, force: true });
    }
  });
});
