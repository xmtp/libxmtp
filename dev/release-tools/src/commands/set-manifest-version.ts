import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";

export function setManifestVersion(
  sdk: string,
  version: string,
  repoRoot: string,
): string {
  const config = getSdkConfig(sdk);
  config.manifest.writeVersion(repoRoot, version);
  return version;
}

export const command = "set-manifest-version";
export const describe = "Set an arbitrary version in an SDK manifest";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Version string to write",
    });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { sdk: string; version: string }>,
) {
  const version = setManifestVersion(argv.sdk, argv.version, argv.repoRoot);
  console.log(version);
}
