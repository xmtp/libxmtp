import { execSync } from "node:child_process";
import type { ArgumentsCamelCase, Argv } from "yargs";
import {
  Sdk,
  BUMP_OPTIONS,
  type BumpOption,
  type BumpType,
  type GlobalArgs,
} from "../types.js";
import { bumpVersion } from "./bump-version.js";
import { scaffoldNotes } from "./scaffold-notes.js";
import { findLastVersion } from "./find-last-version.js";
import { getSdkConfig } from "../lib/sdk-config.js";

function exec(cmd: string, cwd: string): void {
  execSync(cmd, { cwd, stdio: "inherit" });
}

export const command = "create-release-branch";
export const describe =
  "Create a release branch with bumped versions and scaffolded notes";

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
    .option("ios", {
      type: "string",
      default: "none",
      choices: BUMP_OPTIONS,
      describe: "iOS SDK version bump type",
    })
    .option("android", {
      type: "string",
      default: "none",
      choices: BUMP_OPTIONS,
      describe: "Android SDK version bump type",
    });
}

interface CreateReleaseBranchArgs extends GlobalArgs {
  version: string;
  base: string;
  ios: BumpOption;
  android: BumpOption;
}

export function handler(argv: ArgumentsCamelCase<CreateReleaseBranchArgs>) {
  const cwd = argv.repoRoot;
  const branchName = `release/${argv.version}`;

  // Collect SDK bumps to process
  const sdkBumps: Array<{ sdk: Sdk; bump: BumpType }> = [];

  if (argv.ios !== "none") {
    sdkBumps.push({ sdk: Sdk.Ios, bump: argv.ios as BumpType });
  }
  if (argv.android !== "none") {
    sdkBumps.push({ sdk: Sdk.Android, bump: argv.android as BumpType });
  }

  // Validate at least one SDK is being bumped
  if (sdkBumps.length === 0) {
    throw new Error(
      "At least one SDK must be bumped (use --ios or --android with a bump type)",
    );
  }

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  exec(`git checkout -b ${branchName} ${argv.base}`, cwd);

  // Process each SDK
  const bumpedSdks: string[] = [];
  for (const { sdk, bump } of sdkBumps) {
    console.log(`Bumping ${sdk} version (${bump})...`);
    const newVersion = bumpVersion(sdk, bump, cwd);
    console.log(`New ${sdk} version: ${newVersion}`);

    const lastVersion = findLastVersion(sdk, cwd);
    const config = getSdkConfig(sdk);
    const sinceTag = lastVersion ? `${config.tagPrefix}${lastVersion}` : null;

    console.log(`Scaffolding ${sdk} release notes...`);
    const notesPath = scaffoldNotes(sdk, cwd, sinceTag);
    console.log(`Release notes: ${notesPath}`);

    bumpedSdks.push(`${sdk} ${newVersion}`);
  }

  exec("git add -A", cwd);
  exec(
    `git commit -m "chore: create release ${argv.version} (${bumpedSdks.join(", ")})"`,
    cwd,
  );

  console.log(`Branch ${branchName} created and committed.`);
  console.log(`Push with: git push -u origin ${branchName}`);
}
