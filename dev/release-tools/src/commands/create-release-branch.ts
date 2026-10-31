import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";

import { listTags } from "@/lib/git";
import { scaffoldNotes } from "@/lib/scaffold-notes";
import { getSdkConfig } from "@/lib/sdk-config";
import { filterAndSortTags } from "@/lib/version";
import { Sdk, type GlobalArgs } from "@/types";

export const command = "create-release-branch";
export const describe =
  "Create a release branch with the shared SDK version and release notes";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Shared SDK target version and branch name",
    })
    .option("base", {
      type: "string",
      default: "HEAD",
      describe: "Base ref to branch from",
    });
}

interface CreateReleaseBranchArgs extends GlobalArgs {
  version: string;
  base: string;
}

export function handler(argv: ArgumentsCamelCase<CreateReleaseBranchArgs>) {
  const cwd = path.resolve(argv.repoRoot);
  const branchName = `release/${argv.version}`;
  if (
    semver.valid(argv.version) !== argv.version ||
    semver.prerelease(argv.version)
  ) {
    throw new Error("--version must be a stable semver version, such as 8.0.0");
  }
  const git = (args: string[]) =>
    execFileSync("git", args, { cwd, encoding: "utf-8" }).trim();
  if (git(["status", "--porcelain"])) {
    throw new Error("Create a release branch from a clean working tree");
  }

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  git(["checkout", "-b", branchName, argv.base]);
  const sourceCommit = git(["rev-parse", "HEAD"]);

  execFileSync(
    "python3",
    ["-B", path.join(cwd, "dev/sdk_version.py"), "set", argv.version],
    { cwd },
  );
  const bumpedSdks: string[] = [];
  const sdkVersions: Record<string, string> = {};
  for (const sdk of [
    Sdk.Ios,
    Sdk.Android,
    Sdk.NodeSdk,
    Sdk.BrowserSdk,
    Sdk.AgentSdk,
    Sdk.ReactNativeSdk,
    Sdk.Cli,
  ]) {
    const config = getSdkConfig(sdk);
    const newVersion = argv.version;
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
      const sinceTag = previousVersion
        ? `${config.tagPrefix}${previousVersion}`
        : git(["rev-list", "--max-parents=0", "HEAD"]).split("\n")[0];
      notesPath = scaffoldNotes(sdk, cwd, previousVersion ?? null, sinceTag);
    }
    console.log(`Release notes: ${notesPath}`);

    bumpedSdks.push(`${sdk} ${newVersion}`);
    sdkVersions[sdk] = newVersion;
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
