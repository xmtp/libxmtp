import path from "node:path";

import type { ArgumentsCamelCase, Argv } from "yargs";

import { getSdkConfig } from "@/lib/sdk-config";
import { updateSpmChecksum as updateSpmChecksumFn } from "@/lib/spm";
import type { GlobalArgs } from "@/types";

export const command = "update-spm-checksum";
export const describe =
  "Record the shared SwiftPM and CocoaPods archive URL and checksum";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("url", {
      type: "string",
      demandOption: true,
      describe: "Artifact download URL",
    })
    .option("checksum", {
      type: "string",
      demandOption: true,
      describe: "SHA-256 checksum of the artifact",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & {
      sdk: string;
      url: string;
      checksum: string;
    }
  >,
) {
  const config = getSdkConfig(argv.sdk);
  if (!config.spmManifestPath) {
    throw new Error(`SDK ${argv.sdk} does not have an SPM manifest`);
  }

  const spmPath = path.join(argv.repoRoot, config.spmManifestPath);
  updateSpmChecksumFn(spmPath, argv.url, argv.checksum);
  console.log("Updated sdks/ios/ReleaseArtifacts.json");
}
