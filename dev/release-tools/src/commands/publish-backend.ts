import { execFileSync } from "node:child_process";

import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";

import { validateTimestamp } from "@/lib/version";
import type { GlobalArgs } from "@/types";

const IMAGE = "ghcr.io/xmtp/backend";
const RELEASE_TYPES = ["dev", "rc", "final"] as const;

interface PublishBackendArgs extends GlobalArgs {
  version: string;
  releaseType: (typeof RELEASE_TYPES)[number];
  dryRun?: boolean;
}

export const command = "publish-backend";
export const describe = "Publish a backend image version and GitHub release";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("version", { type: "string", demandOption: true })
    .option("releaseType", {
      type: "string",
      choices: RELEASE_TYPES,
      demandOption: true,
    })
    .option("dryRun", { type: "boolean", default: false });
}

function platformDigests(manifest: unknown): string[] {
  if (
    typeof manifest !== "object" ||
    manifest === null ||
    !("manifests" in manifest) ||
    !Array.isArray(manifest.manifests) ||
    manifest.manifests.length !== 2
  ) {
    throw new Error("Backend images require Linux amd64 and arm64 manifests");
  }
  const platforms = new Map<string, string>();
  for (const entry of manifest.manifests as unknown[]) {
    if (typeof entry !== "object" || entry === null)
      throw new Error("Invalid image manifest");
    const record = entry as Record<string, unknown>;
    const platform = record.platform as Record<string, unknown> | undefined;
    if (
      !platform ||
      platform.os !== "linux" ||
      (platform.architecture !== "amd64" &&
        platform.architecture !== "arm64") ||
      typeof record.digest !== "string" ||
      !/^sha256:[a-f0-9]{64}$/.test(record.digest)
    ) {
      throw new Error("Backend images require Linux amd64 and arm64 manifests");
    }
    platforms.set(platform.architecture, record.digest);
  }
  if (platforms.size !== 2)
    throw new Error("Backend images require Linux amd64 and arm64 manifests");
  return [platforms.get("amd64")!, platforms.get("arm64")!];
}

export function handler(argv: ArgumentsCamelCase<PublishBackendArgs>) {
  if (
    semver.valid(argv.version) !== argv.version ||
    semver.parse(argv.version)?.build.length
  ) {
    throw new Error("Backend version must be semver without build metadata");
  }
  const identifiers = semver.prerelease(argv.version) ?? [];
  const suffix = identifiers.join(".");
  const dev =
    /^(?:dev\.[a-f0-9]{7,40}|pre\.[0-9]{14}\.dev\.[a-f0-9]{7,40})$/.test(
      suffix,
    );
  const channelMatches =
    argv.releaseType === "final"
      ? identifiers.length === 0
      : argv.releaseType === "rc"
        ? /^rc[1-9][0-9]*$/.test(suffix)
        : dev;
  if (!channelMatches) {
    throw new Error("Backend version must match the release type");
  }
  if (argv.releaseType === "dev" && identifiers[0] === "pre")
    validateTimestamp(String(identifiers[1]));
  const prerelease = identifiers.length !== 0;
  const run = (program: string, args: string[]) =>
    execFileSync(program, args, {
      cwd: argv.repoRoot,
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  const optional = (
    program: string,
    args: string[],
    missing: RegExp,
  ): string | null => {
    try {
      return run(program, args);
    } catch (error) {
      const value = (error as { stderr?: unknown }).stderr;
      const stderr =
        typeof value === "string"
          ? value
          : Buffer.isBuffer(value)
            ? value.toString("utf8")
            : "";
      if (missing.test(stderr)) return null;
      throw error;
    }
  };
  const sourceSha = run("git", ["rev-parse", "HEAD"]);
  const tag = `backend-${argv.version}`;
  const image = `${IMAGE}:${argv.version}`;
  const sourceImage = `${IMAGE}:sha-${sourceSha}`;
  console.log(
    JSON.stringify(
      {
        version: argv.version,
        tag,
        image,
        sourceImage,
        sourceSha,
        dryRun: !!argv.dryRun,
      },
      null,
      2,
    ),
  );
  if (argv.dryRun) return;

  const remoteRefs = new Map(
    run("git", [
      "ls-remote",
      "--tags",
      "origin",
      `refs/tags/${tag}`,
      `refs/tags/${tag}^{}`,
    ])
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const [sha, ref] = line.split(/\s+/);
        return [ref, sha];
      }),
  );
  const existingSha =
    remoteRefs.get(`refs/tags/${tag}^{}`) ?? remoteRefs.get(`refs/tags/${tag}`);
  if (existingSha && existingSha !== sourceSha)
    throw new Error(`Tag ${tag} belongs to another commit`);

  const source = platformDigests(
    JSON.parse(run("docker", ["manifest", "inspect", sourceImage])),
  );
  for (const digest of source) {
    run("gh", [
      "attestation",
      "verify",
      `oci://${IMAGE}@${digest}`,
      "--repo",
      "xmtp/libxmtp",
      "--signer-workflow",
      "xmtp/libxmtp/.github/workflows/push-backend.yml",
      "--signer-digest",
      sourceSha,
      "--source-digest",
      sourceSha,
    ]);
  }
  const existing = optional(
    "docker",
    ["manifest", "inspect", image],
    /no such manifest|manifest unknown/i,
  );
  if (existing !== null) {
    if (
      JSON.stringify(platformDigests(JSON.parse(existing))) !==
      JSON.stringify(source)
    ) {
      throw new Error(`Image ${image} belongs to another build`);
    }
  } else {
    run("docker", [
      "manifest",
      "create",
      image,
      ...source.map((digest) => `${IMAGE}@${digest}`),
    ]);
    run("docker", ["manifest", "push", "--purge", image]);
  }

  const release = optional(
    "gh",
    ["release", "view", tag, "--json", "tagName"],
    /release not found|HTTP 404/i,
  );
  if (release === null) {
    run("gh", [
      "release",
      "create",
      tag,
      "--target",
      sourceSha,
      "--title",
      `Backend ${argv.version}`,
      "--notes",
      `Backend ${argv.version}\n\nContainer: ${image}\nSource image: ${sourceImage}\nSource commit: ${sourceSha}\nPlatforms: Linux amd64 and arm64.`,
      ...(prerelease ? ["--prerelease"] : []),
    ]);
  }
}
