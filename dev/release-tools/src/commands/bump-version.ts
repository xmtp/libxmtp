import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType, GlobalArgs } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";

export function bumpVersion(
  sdk: string,
  bumpType: BumpType,
  repoRoot: string,
): string {
  const config = getSdkConfig(sdk);
  const currentVersion = config.manifest.readVersion(repoRoot);
  const newVersion = semver.inc(currentVersion, bumpType);
  if (!newVersion) {
    throw new Error(`Failed to bump ${bumpType} on version ${currentVersion}`);
  }
  config.manifest.writeVersion(repoRoot, newVersion);
  return newVersion;
}

export const command = "bump-version";
export const describe = "Bump the version in an SDK manifest";

export function builder(yargs: Argv<GlobalArgs>) {
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
  argv: ArgumentsCamelCase<GlobalArgs & { sdk: string; type: BumpType }>,
) {
  const version = bumpVersion(argv.sdk, argv.type, argv.repoRoot);
  console.log(version);
}
