import type { ArgumentsCamelCase, Argv } from "yargs";
import type { ReleaseType } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";
import { computeVersion as computeVersionFn } from "../lib/version.js";
import { getShortSha } from "../lib/git.js";

export const command = "compute-version";
export const describe =
  "Compute the full version string for a release type";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("release-type", {
      type: "string",
      demandOption: true,
      choices: ["dev", "rc", "final"] as const,
      describe: "Release type",
    })
    .option("rc-number", {
      type: "number",
      describe: "RC number (required for rc releases)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{
    sdk: string;
    releaseType: ReleaseType;
    rcNumber?: number;
  }>
) {
  const config = getSdkConfig(argv.sdk);
  const baseVersion = config.manifest.readVersion(process.cwd());
  const shortSha =
    argv.releaseType === "dev" ? getShortSha(process.cwd()) : undefined;
  const version = computeVersionFn(baseVersion, argv.releaseType, {
    rcNumber: argv.rcNumber,
    shortSha,
  });
  console.log(version);
}
