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
    fs.chmodSync(wrapper, 0o755);
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
      [
        "compute-version",
        "--sdk",
        "libxmtp",
        "--base-version",
        "8.0.0",
        "--release-type",
        "rc",
        "--rc-number",
        "1",
      ],
      "8.0.0-rc1",
    ],
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

  it.each(["release/8.0.0", "refs/heads/release/8.0.0"])(
    "uses the backend branch version from %s",
    (ref) => {
      const workflow = fs.readFileSync(
        path.join(repoRoot, ".github/workflows/release-backend.yml"),
        "utf8",
      );
      const match = workflow.match(
        /- name: Compute version[\s\S]*?        run: \|\n([\s\S]*?)\n  build:/,
      );
      if (!match?.[1]) throw new Error("Backend version script not found");
      const output = path.join(tmpDir, "backend-output");
      fs.writeFileSync(output, "");
      const result = spawnSync(
        "bash",
        ["-c", match[1].replace(/^          /gm, "")],
        {
          cwd: repoRoot,
          env: {
            ...process.env,
            PATH: `${tmpDir}${path.delimiter}${process.env.PATH}`,
            GITHUB_WORKSPACE: repoRoot,
            GITHUB_OUTPUT: output,
            RELEASE_TYPE: "rc",
            RC_NUMBER: "1",
            REF: ref,
            TIMESTAMP: "20261006120000",
          },
          encoding: "utf8",
        },
      );
      expect(result.status, result.stderr).toBe(0);
      expect(fs.readFileSync(output, "utf8")).toBe("version=8.0.0-rc1\n");
    },
  );

  it("rejects a fractional RC number before starting release jobs", () => {
    const workflow = fs.readFileSync(
      path.join(repoRoot, ".github/workflows/release.yml"),
      "utf8",
    );
    const match = workflow.match(
      /- name: Validate RC number[\s\S]*?        run: \|\n([\s\S]*?)\n      - name:/,
    );
    if (!match?.[1]) throw new Error("RC validation script not found");
    const result = spawnSync(
      "bash",
      ["-c", match[1].replace(/^          /gm, "")],
      {
        env: { ...process.env, RC_NUMBER: "1.5" },
        encoding: "utf8",
      },
    );
    expect(result.status).not.toBe(0);
  });

  it("rejects a backend release whose checkout differs from the workflow source", () => {
    const workflow = fs.readFileSync(
      path.join(repoRoot, ".github/workflows/push-backend.yml"),
      "utf8",
    );
    const match = workflow.match(
      /- name: Pin the build source[\s\S]*?        run: \|\n([\s\S]*?)\n  deploy-dev:/,
    );
    if (!match?.[1]) throw new Error("Backend source script not found");
    const output = path.join(tmpDir, "source-output");
    fs.writeFileSync(output, "");
    const result = spawnSync(
      "bash",
      ["-c", match[1].replace(/^          /gm, "")],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          RELEASE_BUILD: "true",
          WORKFLOW_SHA: "0".repeat(40),
          GITHUB_OUTPUT: output,
        },
        encoding: "utf8",
      },
    );
    expect(result.status).not.toBe(0);
    expect(fs.readFileSync(output, "utf8")).toBe("");
  });

  it.each([true, false])(
    "validates the release ref without checking out its code (matching source: %s)",
    (matches) => {
      const fixture = path.join(tmpDir, `source-validation-${matches}`);
      const seed = path.join(fixture, "seed");
      const remote = path.join(fixture, "remote.git");
      const checkout = path.join(fixture, "checkout");
      fs.mkdirSync(fixture, { recursive: true });
      const git = (cwd: string, args: string[]) =>
        execFileSync("git", args, {
          cwd,
          encoding: "utf8",
          stdio: ["ignore", "pipe", "pipe"],
        }).trim();
      git(fixture, ["init", "--initial-branch=trusted", seed]);
      git(seed, ["config", "user.name", "Test"]);
      git(seed, ["config", "user.email", "test@example.com"]);
      git(seed, ["commit", "--allow-empty", "-m", "trusted"]);
      const workflowSha = git(seed, ["rev-parse", "HEAD"]);
      git(seed, ["checkout", "-b", "requested"]);
      git(seed, ["commit", "--allow-empty", "-m", "requested"]);
      git(fixture, ["clone", "--bare", seed, remote]);
      git(fixture, [
        "clone",
        "--branch",
        "trusted",
        `file://${remote}`,
        checkout,
      ]);
      const workflow = fs.readFileSync(
        path.join(repoRoot, ".github/workflows/release-backend.yml"),
        "utf8",
      );
      const script = workflow.match(
        /- name: Pin the release source[\s\S]*?        run: \|\n([\s\S]*?)\n      - uses:/,
      )?.[1];
      if (!script)
        throw new Error("Release source validation script not found");
      const output = path.join(fixture, "output");
      fs.writeFileSync(output, "");
      const result = spawnSync(
        "bash",
        ["-c", script.replace(/^          /gm, "")],
        {
          cwd: checkout,
          env: {
            ...process.env,
            REF: matches ? "refs/heads/trusted" : "requested",
            WORKFLOW_SHA: workflowSha,
            GITHUB_OUTPUT: output,
          },
          encoding: "utf8",
        },
      );
      expect(result.status, result.stderr).toBe(matches ? 0 : 1);
      expect(fs.readFileSync(output, "utf8")).toBe(
        matches ? `sha=${workflowSha}\n` : "",
      );
      expect(git(checkout, ["rev-parse", "HEAD"])).toBe(workflowSha);
    },
  );

  it("runs backend setup tools only after validation with a read-only token and trusted checkout", () => {
    const workflow = fs.readFileSync(
      path.join(repoRoot, ".github/workflows/release-backend.yml"),
      "utf8",
    );
    const setup = workflow.match(/\n  setup:\n([\s\S]*?)\n  build:/)?.[1];
    if (!setup) throw new Error("Backend setup job not found");
    const permissions = setup.match(
      /    permissions:\n([\s\S]*?)    outputs:/,
    )?.[1];
    expect(permissions?.trim()).toBe("contents: read");
    expect(
      setup.match(
        /uses: actions\/checkout@[^\n]+\n        with:\n          ref: ([^\n]+)/,
      )?.[1],
    ).toBe("${{ github.sha }}");
    expect(setup.indexOf("- name: Pin the release source")).toBeLessThan(
      setup.indexOf("- uses: ./.github/actions/setup-release-tools"),
    );
  });

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

  it.each([0, 1])(
    "captures the local push digest and preserves Docker exit status %s",
    (pushStatus) => {
      const workflow = fs.readFileSync(
        path.join(repoRoot, ".github/workflows/push-backend.yml"),
        "utf8",
      );
      const match = workflow.match(
        /- name: Publish architecture image[\s\S]*?        run: \|\n([\s\S]*?)\n      - name: Attest/,
      );
      if (!match?.[1])
        throw new Error("Backend image publish script not found");
      const digest = `sha256:${"a".repeat(64)}`;
      fs.writeFileSync(
        path.join(tmpDir, "docker"),
        `#!/bin/bash\nif [ "$1" = push ]; then\n  echo 'image: digest: ${digest} size: 1234'\n  exit ${pushStatus}\nfi\n`,
      );
      fs.chmodSync(path.join(tmpDir, "docker"), 0o755);
      const output = path.join(tmpDir, "digest-output");
      fs.writeFileSync(output, "");
      const result = spawnSync(
        "bash",
        ["-c", match[1].replace(/^          /gm, "")],
        {
          env: {
            ...process.env,
            PATH: `${tmpDir}${path.delimiter}${process.env.PATH}`,
            COMMIT_SHA: "a".repeat(40),
            ARCH: "amd64",
            RUNNER_TEMP: tmpDir,
            GITHUB_OUTPUT: output,
          },
          encoding: "utf8",
        },
      );
      expect(result.status).toBe(pushStatus);
      expect(fs.readFileSync(output, "utf8")).toBe(
        pushStatus ? "" : `digest=${digest}\n`,
      );
    },
  );
});
