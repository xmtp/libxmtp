import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { handler } from "../../src/commands/create-release-branch";
import { getRepoRoot } from "../../src/lib/git";

describe("shared release branch", () => {
  let root: string;
  const git = (...args: string[]) =>
    execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
  const prepare = () =>
    handler({
      _: [],
      $0: "test",
      repoRoot: root,
      version: "8.1.0",
      base: "HEAD",
    });
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "shared-release-"));
    for (const file of [
      "Cargo.toml",
      "Cargo.lock",
      "dev/sdk_version.py",
      "crates/xmtp_sdk/Cargo.toml",
      "sdks/android/gradle.properties",
      ...["node", "browser", "agent", "react-native"].map(
        (sdk) => `sdks/${sdk}/package.json`,
      ),
    ]) {
      fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
      fs.copyFileSync(path.join(getRepoRoot(), file), path.join(root, file));
    }
    execFileSync("python3", ["-B", "dev/sdk_version.py", "set", "8.0.0"], {
      cwd: root,
    });
    git("init", "-q");
    git("config", "user.email", "fixture@example.invalid");
    git("config", "user.name", "fixture");
    git("config", "commit.gpgsign", "false");
    git("config", "tag.gpgsign", "false");
    git("add", ".");
    git("commit", "-qm", "fixture");
  });
  afterEach(() => fs.rmSync(root, { recursive: true, force: true }));

  it("sets Cargo, every SDK, the lock, and the branch to one target", () => {
    prepare();
    expect(git("branch", "--show-current")).toBe("release/8.1.0");
    expect(git("status", "--porcelain")).toBe("");
    expect(fs.readFileSync(path.join(root, "Cargo.toml"), "utf8")).toContain(
      'version = "8.1.0"',
    );
    expect(fs.readFileSync(path.join(root, "Cargo.lock"), "utf8")).toContain(
      'name = "xmtp_sdk"\nversion = "8.1.0"',
    );
    const record = JSON.parse(
      fs.readFileSync(
        path.join(root, "docs/release-notes/release-8.1.0.json"),
        "utf8",
      ),
    );
    expect(Object.values(record.sdks)).toEqual(Array(6).fill("8.1.0"));
    for (const sdk of ["node", "browser", "agent", "react-native"])
      expect(
        JSON.parse(
          fs.readFileSync(path.join(root, `sdks/${sdk}/package.json`), "utf8"),
        ).version,
      ).toBe("8.1.0");
  });
  it("keeps reviewed notes and selects only stable baselines for missing notes", () => {
    const note = path.join(root, "docs/release-notes/ios/8.1.0.md");
    fs.mkdirSync(path.dirname(note), { recursive: true });
    fs.writeFileSync(note, "Reviewed notes\n");
    git("add", ".");
    git("commit", "-qm", "notes");
    git("tag", "android-7.0.0");
    git("tag", "android-8.1.0-rc4");
    prepare();
    expect(fs.readFileSync(note, "utf8")).toBe("Reviewed notes\n");
    expect(
      fs.readFileSync(
        path.join(root, "docs/release-notes/android/8.1.0.md"),
        "utf8",
      ),
    ).toContain('previous_release_tag = "android-7.0.0"');
  });
  it("refuses uncommitted changes", () => {
    fs.writeFileSync(path.join(root, "local"), "uncommitted");
    expect(prepare).toThrow("clean working tree");
    expect(git("branch", "--show-current")).not.toBe("release/8.1.0");
  });
});
