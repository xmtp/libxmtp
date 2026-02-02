import path from "node:path";
import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { updateSpmChecksum as updateSpmChecksumFn } from "../lib/spm.js";

export const command = "update-spm-checksum";
export const describe =
  "Update the binary target URL and checksum in Package.swift";

export function builder(yargs: Argv) {
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
  argv: ArgumentsCamelCase<{
    sdk: string;
    url: string;
    checksum: string;
  }>
) {
  const config = getSdkConfig(argv.sdk);
  if (!config.spmManifestPath) {
    throw new Error(`SDK ${argv.sdk} does not have an SPM manifest`);
  }
  const spmPath = path.join(process.cwd(), config.spmManifestPath);
  updateSpmChecksumFn(spmPath, argv.url, argv.checksum);
  console.log(`Updated ${config.spmManifestPath}`);
}
