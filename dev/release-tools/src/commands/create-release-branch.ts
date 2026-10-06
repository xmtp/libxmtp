import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";

import { listTags, tagExists } from "@/lib/git";
import { getSdkConfig } from "@/lib/sdk-config";
import { filterAndSortTags } from "@/lib/version";
import { Sdk, BUMP_OPTIONS, type BumpType, type GlobalArgs } from "@/types";

import { bumpVersion } from "./bump-version";
import { scaffoldNotes } from "./scaffold-notes";
import { setManifestVersion } from "./set-manifest-version";

type SdkBump = {
  sdk: Sdk;
  bump: BumpType | "keep";
};

export const command = "create-release-branch";
export const describe =
  "Create a release branch with selected SDK versions and release notes";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Release version number (used in branch name)",
    })
    .option("base", {
      type: "string",
      default: "HEAD",
      describe: "Base ref to branch from",
    })
    .option("libxmtp-version", {
      type: "string",
      describe:
        "Set the Rust workspace version (default: keep its current version)",
    })
    .option("ios", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "iOS SDK version bump type",
    })
    .option("android", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "Android SDK version bump type",
    })
    .option("node-sdk", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "Node SDK version bump type",
    })
    .option("browser-sdk", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "Browser SDK version bump type",
    })
    .option("agent-sdk", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "Agent SDK version bump type",
    })
    .option("cli", {
      type: "string",
      default: "none",
      choices: [...BUMP_OPTIONS, "keep"],
      describe: "CLI version bump type",
    });
}

interface CreateReleaseBranchArgs extends GlobalArgs {
  version: string;
  base: string;
  ios: string;
  android: string;
  nodeSdk?: string;
  browserSdk?: string;
  agentSdk?: string;
  cli?: string;
  libxmtpVersion?: string;
}

export function handler(argv: ArgumentsCamelCase<CreateReleaseBranchArgs>) {
  const cwd = argv.repoRoot;
  const branchName = `release/${argv.version}`;
  if (!semver.valid(argv.version) || semver.prerelease(argv.version)) {
    throw new Error("--version must be a stable semver version, such as 8.0.0");
  }
  if (argv.libxmtpVersion && !semver.valid(argv.libxmtpVersion)) {
    throw new Error("--libxmtp-version must be a valid semver version");
  }
  const git = (args: string[]) =>
    execFileSync("git", args, { cwd, encoding: "utf-8" }).trim();
  if (git(["status", "--porcelain"])) {
    throw new Error("Create a release branch from a clean working tree");
  }

  // Collect SDK bumps to process
  const sdkBumps: Array<SdkBump> = [];

  if (argv.ios !== "none") {
    sdkBumps.push({ sdk: Sdk.Ios, bump: argv.ios as SdkBump["bump"] });
  }
  if (argv.android !== "none") {
    sdkBumps.push({ sdk: Sdk.Android, bump: argv.android as SdkBump["bump"] });
  }
  if (argv.nodeSdk && argv.nodeSdk !== "none") {
    sdkBumps.push({ sdk: Sdk.NodeSdk, bump: argv.nodeSdk as SdkBump["bump"] });
  }
  if (argv.browserSdk && argv.browserSdk !== "none") {
    sdkBumps.push({
      sdk: Sdk.BrowserSdk,
      bump: argv.browserSdk as SdkBump["bump"],
    });
  }
  if (argv.agentSdk && argv.agentSdk !== "none") {
    sdkBumps.push({
      sdk: Sdk.AgentSdk,
      bump: argv.agentSdk as SdkBump["bump"],
    });
  }
  if (argv.cli && argv.cli !== "none") {
    sdkBumps.push({ sdk: Sdk.Cli, bump: argv.cli as SdkBump["bump"] });
  }

  // Validate at least one SDK is being released
  if (sdkBumps.length === 0) {
    throw new Error(
      "Select at least one SDK with keep, patch, minor, or major",
    );
  }

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  git(["checkout", "-b", branchName, argv.base]);
  const sourceCommit = git(["rev-parse", "HEAD"]);

  // Process each SDK
  const bumpedSdks: string[] = [];
  const sdkVersions: Record<string, string> = {};
  for (const { sdk, bump } of sdkBumps) {
    const config = getSdkConfig(sdk);
    const currentVersion = config.manifest.readVersion(cwd);
    const newVersion =
      bump === "keep" ? currentVersion : bumpVersion(sdk, bump, cwd);
    console.log(`New ${sdk} version: ${newVersion}`);
    const sdkName = config.tagPrefix.replace(/-$/, "");
    let notesPath = path.join(
      cwd,
      "docs/release-notes",
      sdkName,
      `${newVersion}.md`,
    );
    if (!fs.existsSync(notesPath)) {
      const previousVersion = filterAndSortTags(
        listTags(cwd),
        config.tagPrefix,
        config.artifactTagSuffix,
      ).find((version) => semver.lt(version, newVersion));
      const candidateTag = `${config.tagPrefix}${currentVersion}`;
      const sinceTag = previousVersion
        ? `${config.tagPrefix}${previousVersion}`
        : bump === "keep"
          ? git(["rev-list", "--max-parents=0", "HEAD"]).split("\n")[0]
          : tagExists(cwd, candidateTag)
            ? candidateTag
            : null;
      notesPath = scaffoldNotes(
        sdk,
        cwd,
        previousVersion ?? (bump === "keep" ? null : currentVersion),
        sinceTag,
      );
    }
    console.log(`Release notes: ${notesPath}`);

    bumpedSdks.push(`${sdk} ${newVersion}`);
    sdkVersions[sdk] = newVersion;
  }

  if (argv.libxmtpVersion) {
    console.log(`Setting libxmtp version to ${argv.libxmtpVersion}...`);
    setManifestVersion("libxmtp", argv.libxmtpVersion, cwd);
  }

  const releaseRecord = {
    version: argv.version,
    sourceCommit,
    libxmtpVersion: getSdkConfig(Sdk.Libxmtp).manifest.readVersion(cwd),
    sdks: sdkVersions,
  };
  const recordPath = path.join(
    cwd,
    "docs/release-notes",
    `release-${argv.version}.json`,
  );
  fs.writeFileSync(recordPath, JSON.stringify(releaseRecord, null, 2) + "\n");

  git(["add", "-A"]);
  git([
    "commit",
    "-m",
    `chore: create release ${argv.version} (${bumpedSdks.join(", ")})`,
  ]);

  console.log(`Branch ${branchName} created and committed.`);
  console.log(`Push with: git push -u origin ${branchName}`);
}
