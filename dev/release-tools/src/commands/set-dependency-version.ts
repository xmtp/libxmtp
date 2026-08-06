import path from "node:path";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import { setPackageJsonDependency } from "../lib/manifest";

export const command = "set-dependency-version";
export const describe =
  "Rewrite a single dependency's version spec in an SDK's package.json manifest";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. node-sdk)",
    })
    .option("dep", {
      type: "string",
      demandOption: true,
      describe: "Dependency name to rewrite (e.g. @xmtp/node-bindings)",
    })
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "New version spec to write (e.g. 1.11.0-nightly.20260604.abc)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<
    GlobalArgs & { sdk: string; dep: string; version: string }
  >,
) {
  const config = getSdkConfig(argv.sdk);
  const packageJsonPath = path.join(argv.repoRoot, config.manifestPath);
  setPackageJsonDependency(packageJsonPath, argv.dep, argv.version);
  console.log(`Set ${argv.dep} to ${argv.version} in ${argv.sdk}`);
}
