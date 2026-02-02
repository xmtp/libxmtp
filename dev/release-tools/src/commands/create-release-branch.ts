import { execSync } from "node:child_process";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType } from "../types.js";
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

export function builder(yargs: Argv) {
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
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK to bump (e.g. ios)",
    })
    .option("bump", {
      type: "string",
      demandOption: true,
      choices: ["major", "minor", "patch"] as const,
      describe: "Version bump type",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{
    version: string;
    base: string;
    sdk: string;
    bump: BumpType;
  }>
) {
  const cwd = process.cwd();
  const branchName = `release/${argv.version}`;

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  exec(`git checkout -b ${branchName} ${argv.base}`, cwd);

  console.log(`Bumping ${argv.sdk} version (${argv.bump})...`);
  const newVersion = bumpVersion(argv.sdk, argv.bump, cwd);
  console.log(`New ${argv.sdk} version: ${newVersion}`);

  const config = getSdkConfig(argv.sdk);
  const lastVersion = findLastVersion(argv.sdk, cwd);
  const sinceTag = lastVersion
    ? `${config.tagPrefix}${lastVersion}`
    : null;
  console.log(`Scaffolding release notes...`);
  const notesPath = scaffoldNotes(argv.sdk, cwd, sinceTag);
  console.log(`Release notes: ${notesPath}`);

  exec("git add -A", cwd);
  exec(
    `git commit -m "chore: create release ${argv.version} with ${argv.sdk} ${newVersion}"`,
    cwd
  );

  console.log(`Branch ${branchName} created and committed.`);
  console.log(`Push with: git push -u origin ${branchName}`);
}
