import { execFileSync } from "node:child_process";

import { beforeEach, describe, expect, it, vi } from "vitest";

import { handler } from "../../src/commands/publish-backend";

vi.mock("node:child_process", () => ({ execFileSync: vi.fn() }));

const SHA = "a".repeat(40);
const IMAGE = "ghcr.io/xmtp/backend:8.0.0-rc1";
const manifest = {
  manifests: [
    {
      digest: `sha256:${"b".repeat(64)}`,
      platform: { os: "linux", architecture: "amd64" },
    },
    {
      digest: `sha256:${"c".repeat(64)}`,
      platform: { os: "linux", architecture: "arm64" },
    },
  ],
};
const args = {
  repoRoot: "/repo",
  version: "8.0.0-rc1",
  releaseType: "rc" as const,
  _: [],
  $0: "test",
};
let remoteTag: string;
let source: typeof manifest;
let existingImage: typeof manifest | null;
let imageError: string;
let pushFails: boolean;
let releaseExists: boolean;
let failedAttestation: string;

function failure(stderr: string) {
  return Object.assign(new Error(stderr), { stderr });
}
function writes() {
  return vi
    .mocked(execFileSync)
    .mock.calls.filter(
      ([program, command]) =>
        (program === "docker" &&
          (command?.[1] === "create" || command?.[1] === "push")) ||
        (program === "gh" && command?.[1] === "create"),
    );
}

beforeEach(() => {
  remoteTag = "";
  source = structuredClone(manifest);
  existingImage = null;
  imageError = "no such manifest";
  pushFails = false;
  releaseExists = false;
  failedAttestation = "";
  vi.mocked(execFileSync).mockReset();
  vi.mocked(execFileSync).mockImplementation((program, command) => {
    const cmd = command as string[];
    if (program === "git") return cmd[0] === "rev-parse" ? SHA : remoteTag;
    if (program === "docker" && cmd[1] === "inspect") {
      if (cmd[2].includes(":sha-")) return JSON.stringify(source);
      if (existingImage) return JSON.stringify(existingImage);
      throw failure(imageError);
    }
    if (program === "docker" && cmd[1] === "push" && pushFails)
      throw failure("push failed");
    if (program === "gh" && cmd[1] === "view" && !releaseExists)
      throw failure("release not found");
    if (
      program === "gh" &&
      cmd[0] === "attestation" &&
      cmd[2] === failedAttestation
    )
      throw failure("provenance verification failed");
    return "";
  });
});

describe("publish-backend", () => {
  it("publishes pinned platform digests before creating the RC tag", () => {
    handler(args);
    expect(
      writes().map(([program, command]) => [program, ...(command ?? [])]),
    ).toEqual([
      [
        "docker",
        "manifest",
        "create",
        IMAGE,
        ...manifest.manifests.map(
          ({ digest }) => `ghcr.io/xmtp/backend@${digest}`,
        ),
      ],
      ["docker", "manifest", "push", "--purge", IMAGE],
      [
        "gh",
        "release",
        "create",
        "backend-8.0.0-rc1",
        "--target",
        SHA,
        "--title",
        "Backend 8.0.0-rc1",
        "--notes",
        expect.stringContaining(`Source commit: ${SHA}`),
        "--prerelease",
      ],
    ]);
  });

  it("verifies repository, workflow, and source provenance for both platform digests", () => {
    handler(args);
    const calls = vi.mocked(execFileSync).mock.calls;
    for (const { digest } of manifest.manifests) {
      expect(calls).toContainEqual([
        "gh",
        [
          "attestation",
          "verify",
          `oci://ghcr.io/xmtp/backend@${digest}`,
          "--repo",
          "xmtp/libxmtp",
          "--signer-workflow",
          "xmtp/libxmtp/.github/workflows/push-backend.yml",
          "--signer-digest",
          SHA,
          "--source-digest",
          SHA,
        ],
        expect.anything(),
      ]);
    }
    const lastVerification = calls.findLastIndex(
      ([, command]) => command?.[0] === "attestation",
    );
    const firstWrite = calls.findIndex((call) => writes().includes(call));
    expect(lastVerification).toBeLessThan(firstWrite);
  });

  it.each([0, 1])(
    "rejects an untrusted platform digest at index %s before publication",
    (index) => {
      source.manifests[index].digest = `sha256:${"e".repeat(64)}`;
      failedAttestation = `oci://ghcr.io/xmtp/backend@${source.manifests[index].digest}`;
      expect(() => handler(args)).toThrow("provenance verification failed");
      expect(writes()).toEqual([]);
    },
  );

  it.each([
    ["8.0.0-dev.abcdef1", "rc"],
    ["8.0.0-pre.20261006120000.dev.abcdef1", "rc"],
    ["8.0.0-rc1", "dev"],
    ["8.0.0-pre.20261006120000.nightly.abcdef1", "dev"],
    ["8.0.0-rc0", "rc"],
    ["8.0.0-rc01", "rc"],
    ["8.0.0-rc1.extra", "rc"],
    ["8.0.0-dev.abc", "dev"],
    ["8.0.0-dev.notasha", "dev"],
    ["8.0.0-pre.20261301120000.dev.abcdef1", "dev"],
  ] as const)(
    "rejects version %s for release type %s",
    (version, releaseType) => {
      expect(() => handler({ ...args, version, releaseType })).toThrow();
      expect(vi.mocked(execFileSync)).not.toHaveBeenCalled();
    },
  );

  it.each(["8.0.0-dev.abcdef1", "8.0.0-pre.20261006120000.dev.abcdef1"])(
    "publishes supported dev version %s as a prerelease",
    (version) => {
      handler({ ...args, version, releaseType: "dev" });
      const create = writes().find(([program]) => program === "gh");
      expect(create?.[1]).toContain(`backend-${version}`);
      expect(create?.[1]).toContain("--prerelease");
    },
  );

  it("does not contact registries or GitHub in a dry run", () => {
    handler({ ...args, dryRun: true });
    expect(
      vi
        .mocked(execFileSync)
        .mock.calls.map(([program, command]) => [program, ...(command ?? [])]),
    ).toEqual([["git", "rev-parse", "HEAD"]]);
  });

  it("refuses a Git tag from another commit before publication", () => {
    remoteTag = `${"d".repeat(40)}\trefs/tags/backend-8.0.0-rc1`;
    expect(() => handler(args)).toThrow("belongs to another commit");
    expect(writes()).toEqual([]);
  });

  it("refuses an image version with different platform digests", () => {
    existingImage = structuredClone(manifest);
    existingImage.manifests[0].digest = `sha256:${"d".repeat(64)}`;
    expect(() => handler(args)).toThrow("belongs to another build");
    expect(writes()).toEqual([]);
  });

  it("requires both Linux architectures", () => {
    source.manifests.pop();
    expect(() => handler(args)).toThrow("amd64 and arm64");
    expect(writes()).toEqual([]);
  });

  it("does not treat a registry failure as a missing version", () => {
    imageError = "registry connection failed";
    expect(() => handler(args)).toThrow(imageError);
    expect(writes()).toEqual([]);
  });

  it("does not create a release after image publication fails", () => {
    pushFails = true;
    expect(() => handler(args)).toThrow("push failed");
    expect(writes().filter(([program]) => program === "gh")).toEqual([]);
  });

  it("retries an existing annotated tag and matching image without writes", () => {
    remoteTag = `${"d".repeat(40)}\trefs/tags/backend-8.0.0-rc1\n${SHA}\trefs/tags/backend-8.0.0-rc1^{}`;
    existingImage = structuredClone(manifest);
    releaseExists = true;
    handler(args);
    expect(vi.mocked(execFileSync).mock.calls).toEqual(
      expect.arrayContaining([
        ["docker", ["manifest", "inspect", IMAGE], expect.anything()],
        [
          "gh",
          ["release", "view", "backend-8.0.0-rc1", "--json", "tagName"],
          expect.anything(),
        ],
      ]),
    );
    expect(writes()).toEqual([]);
  });

  it("creates a final release without the prerelease flag", () => {
    handler({ ...args, version: "8.0.0", releaseType: "final" });
    const create = writes().find(([program]) => program === "gh");
    expect(create?.[1]).toContain("backend-8.0.0");
    expect(create?.[1]).not.toContain("--prerelease");
  });

  it.each(["not-semver", "1.12.0+build", "8.0.0"])(
    "rejects an invalid RC version %s",
    (version) => {
      expect(() => handler({ ...args, version })).toThrow();
      expect(writes()).toEqual([]);
    },
  );
});
