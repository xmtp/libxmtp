import type { ArgumentsCamelCase, Argv } from "yargs";
import semver from "semver";
import type { GlobalArgs } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import { createTag, pushTag } from "../lib/git";

export const command = "tag-release";
export const describe = "Create and push a git tag for an SDK release";

export function buildTag(sdk: string, version: string): string {
  if (!version || !semver.parse(version)) {
    throw new Error(
      `Invalid version: "${version}". Must be a valid semver string.`,
    );
  }
  const config = getSdkConfig(sdk);
  return `${config.tagPrefix}${version}`;
}

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios, android)",
    })
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Version string for the tag",
    })
    .option("pushBranch", {
      type: "boolean",
      default: false,
      describe: "Also push the current branch (HEAD) along with the tag",
    })
    .option("ignoreIfExists", {
      type: "boolean",
      default: false,
      describe:
        "Skip tag creation/push if the tag already exists (useful for retries)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      sdk: string;
      version: string;
      pushBranch: boolean;
      ignoreIfExists: boolean;
    }
  >,
) {
  const tag = buildTag(argv.sdk, argv.version);
  createTag(argv.repoRoot, tag, argv.ignoreIfExists);
  pushTag(argv.repoRoot, tag, argv.pushBranch, argv.ignoreIfExists);
  console.log(tag);
}
