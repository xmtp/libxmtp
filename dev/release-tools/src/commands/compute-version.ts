import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs, ReleaseType } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";
import { computeVersion as computeVersionFn } from "../lib/version.js";
import { getShortSha } from "../lib/git.js";

export const command = "compute-version";
export const describe = "Compute the full version string for a release type";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("releaseType", {
      type: "string",
      demandOption: true,
      choices: ["dev", "rc", "final"] as const,
      describe: "Release type",
    })
    .option("rcNumber", {
      type: "number",
      describe: "RC number (required for rc releases)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & { sdk: string; releaseType: ReleaseType; rcNumber?: number }
  >,
) {
  if (argv.releaseType === "rc" && argv.rcNumber == null) {
    throw new Error("--rc-number is required when --release-type is 'rc'");
  }

  const config = getSdkConfig(argv.sdk);
  const baseVersion = config.manifest.readVersion(argv.repoRoot);
  const shortSha =
    argv.releaseType === "dev" ? getShortSha(argv.repoRoot) : undefined;
  const version = computeVersionFn(baseVersion, argv.releaseType, {
    rcNumber: argv.rcNumber,
    shortSha,
  });
  console.log(version);
}
