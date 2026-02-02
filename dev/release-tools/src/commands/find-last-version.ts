import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { filterAndSortTags } from "../lib/version.js";
import { listTags } from "../lib/git.js";

export function findLastVersion(
  sdk: string,
  cwd: string,
  preRelease = false,
): string | null {
  const config = getSdkConfig(sdk);
  const tags = listTags(cwd);
  const versions = filterAndSortTags(
    tags,
    config.tagPrefix,
    config.artifactTagSuffix,
    preRelease,
  );
  return versions.length > 0 ? versions[0] : null;
}

export const command = "find-last-version";
export const describe = "Find the latest published version for an SDK";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("pre-release", {
      type: "boolean",
      default: false,
      describe: "Include prerelease versions",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{ sdk: string; preRelease: boolean }>,
) {
  const version = findLastVersion(argv.sdk, process.cwd(), argv.preRelease);
  if (version) {
    console.log(version);
  } else {
    console.log("");
  }
}
