import path from "node:path";
import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";
import { readPodspecVersion, writePodspecVersion } from "../lib/manifest.js";

export function bumpVersion(
  sdk: string,
  bumpType: BumpType,
  repoRoot: string
): string {
  const config = getSdkConfig(sdk);
  const manifestPath = path.join(repoRoot, config.manifestPath);
  const currentVersion = readPodspecVersion(manifestPath);
  const newVersion = semver.inc(currentVersion, bumpType);
  if (!newVersion) {
    throw new Error(
      `Failed to bump ${bumpType} on version ${currentVersion}`
    );
  }
  writePodspecVersion(manifestPath, newVersion);
  return newVersion;
}

export const command = "bump-version";
export const describe = "Bump the version in an SDK manifest";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("type", {
      type: "string",
      demandOption: true,
      choices: ["major", "minor", "patch"] as const,
      describe: "Version bump type",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{ sdk: string; type: BumpType }>
) {
  const version = bumpVersion(argv.sdk, argv.type, process.cwd());
  console.log(version);
}
