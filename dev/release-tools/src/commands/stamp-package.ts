import path from "node:path";

import type { ArgumentsCamelCase, Argv } from "yargs";

import { stampPackage } from "@/lib/stamp-package";
import type { GlobalArgs } from "@/types";

export const command = "stamp-package";
export const describe =
  "Set the version of a staged SDK product and update its checksum receipt";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("dir", {
      type: "string",
      demandOption: true,
      describe:
        "Staged product directory, absolute or relative to the repository root",
    })
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "npm version to publish (e.g. 8.0.1-rc.1)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { dir: string; version: string }>,
) {
  // The CLI runs in its package directory; a relative path names a repository path.
  stampPackage(path.resolve(argv.repoRoot, argv.dir), argv.version);
  console.log(`Set ${argv.dir} to version ${argv.version}`);
}
