import type { ArgumentsCamelCase, Argv } from "yargs";

import { getSdkConfig } from "@/lib/sdk-config";
import type { GlobalArgs } from "@/types";

export function setManifestVersion(
  sdk: string,
  version: string,
  repoRoot: string,
): string {
  const config = getSdkConfig(sdk);
  if (sdk !== "cli")
    throw new Error("SDK versions are shared; use just version set");
  config.manifest.writeVersion(repoRoot, version);
  return version;
}

export const command = "set-manifest-version";
export const describe = "Set the separately versioned CLI application version";

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
