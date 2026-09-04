import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { handler } from "../../src/commands/compute-version";

// The handler resolves the short sha through git, so exercise it against a
// real throwaway repo — same approach as the other command tests.
describe("compute-version --source-ref / --timestamp", () => {
  let tmpDir: string;
  let shortSha: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-compute-"));
    fs.mkdirSync(path.join(tmpDir, "sdks/js/node-sdk"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/js/node-sdk/package.json"),
      `${JSON.stringify({ name: "@xmtp/node-sdk", version: "6.2.0" }, null, 2)}\n`,
    );
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    execSync("git config commit.gpgSign false", { cwd: tmpDir });
    execSync("git add -A", { cwd: tmpDir });
    execSync('git commit -m "init"', { cwd: tmpDir });
    shortSha = execSync("git rev-parse --short=7 HEAD", { cwd: tmpDir })
      .toString()
      .trim();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function run(sourceRef?: string, timestamp?: string): string {
    const spy = vi.spyOn(console, "log").mockImplementation(() => {});
    try {
      handler({
        _: [],
        $0: "",
        repoRoot: tmpDir,
        sdk: "node-sdk",
        releaseType: "dev",
        sourceRef,
        timestamp,
      } as Parameters<typeof handler>[0]);
      return String(spy.mock.calls.at(-1)?.[0]);
    } finally {
      spy.mockRestore();
    }
  }

  it("emits the ordered pre.* shape for a main-cut dev", () => {
    const version = run("refs/heads/main");
    expect(version).toMatch(
      new RegExp(`^6\\.2\\.0-pre\\.\\d{14}\\.dev\\.${shortSha}$`),
    );
  });

  it("emits the legacy shape without --source-ref", () => {
    expect(run()).toBe(`6.2.0-dev.${shortSha}`);
  });

  it("treats a branch-cut ref as legacy", () => {
    expect(run("refs/heads/release/1.10")).toBe(`6.2.0-dev.${shortSha}`);
  });

  it("accepts a bare 'main' as well as refs/heads/main", () => {
    expect(run("main")).toMatch(/^6\.2\.0-pre\.\d{14}\.dev\.[0-9a-f]{7}$/);
  });

  // The run-wide stamp release.yml mints must reach the version verbatim —
  // that identity is what makes every SDK of one run share a pre.* suffix.
  it("uses the supplied --timestamp verbatim", () => {
    expect(run("refs/heads/main", "20260102030405")).toBe(
      `6.2.0-pre.20260102030405.dev.${shortSha}`,
    );
  });

  it("rejects a malformed --timestamp", () => {
    expect(() => run("refs/heads/main", "2026")).toThrow(/timestamp/);
  });

  // A 12-digit stamp is the old minute-precision shape. Accepting it would let
  // a stale caller reintroduce same-minute ordering ties, so it must fail loud
  // rather than pass through.
  it("rejects a minute-precision (12-digit) --timestamp", () => {
    expect(() => run("refs/heads/main", "202601020304")).toThrow(/14 digits/);
  });

  // 14 digits that name no instant would still be interpolated verbatim into
  // the published version, where they either sort by fiction (month 13) or
  // make the version unparseable (leading zero). Neither reaches npm.
  it.each(["20261301000000", "20260230120000", "00000000000000"])(
    "rejects the impossible calendar stamp %s",
    (stamp) => {
      expect(() => run("refs/heads/main", stamp)).toThrow(/real UTC/);
    },
  );
});
