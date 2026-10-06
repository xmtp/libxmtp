import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import semver from "semver";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { getSdkConfig } from "../src/lib/sdk-config";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const shortSha = execFileSync("git", ["rev-parse", "--short=7", "HEAD"], {
  cwd: repoRoot,
  encoding: "utf8",
}).trim();

function baseVersion(sdk: string): string {
  const version = semver.parse(
    getSdkConfig(sdk).manifest.readVersion(repoRoot),
  );
  if (!version) throw new Error(`Invalid manifest version for ${sdk}`);
  return `${version.major}.${version.minor}.${version.patch}`;
}

describe("release action CLI wrapper", () => {
  let tmpDir: string;
  let wrapper: string;

  beforeAll(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-wrapper-"));
    wrapper = path.join(tmpDir, "xmtp-release");
    const action = fs.readFileSync(
      path.join(repoRoot, ".github/actions/setup-release-tools/action.yml"),
      "utf8",
    );
    // Run the same wrapper that the composite action installs in CI.
    const body = action.match(/<< 'WRAPPER'\n([\s\S]*?)        WRAPPER/);
    if (!body?.[1]) throw new Error("Release action wrapper not found");
    fs.writeFileSync(wrapper, body[1].replace(/^        /gm, ""));
  });

  afterAll(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function run(args: string[]) {
    return spawnSync("bash", [wrapper, ...args], {
      cwd: tmpDir,
      env: { ...process.env, GITHUB_WORKSPACE: repoRoot },
      encoding: "utf8",
    });
  }

  it.each([
    [
      ["compute-version", "--sdk", "node-sdk", "--release-type", "final"],
      baseVersion("node-sdk"),
    ],
    [
      [
        "compute-version",
        "--sdk",
        "android",
        "--release-type",
        "dev",
        "--source-ref",
        "self-hosted",
      ],
      `${baseVersion("android")}-dev.${shortSha}`,
    ],
    [
      [
        "resolve-sdk-version",
        "--sdk",
        "node-sdk",
        "--release-type",
        "nightly",
        "--timestamp",
        "20260102030405",
        "--pending-version",
        "1.11.0",
        "--pending-kind",
        "patch",
      ],
      `${semver.inc(baseVersion("node-sdk"), "patch")}-pre.20260102030405.nightly.${shortSha}`,
    ],
  ] satisfies [string[], string][])(
    "emits only the version for %j",
    (args, expected) => {
      const result = run(args);
      expect(result.status, result.stderr).toBe(0);
      expect(result.stdout).toBe(`${expected}\n`);
    },
  );

  it("keeps command errors and a failed exit status", () => {
    const result = run([
      "compute-version",
      "--sdk",
      "node-sdk",
      "--release-type",
      "rc",
    ]);
    expect(result.status).not.toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain("--rc-number is required");
  });
});
