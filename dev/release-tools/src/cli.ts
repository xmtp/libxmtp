#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";

import * as classifyNotes from "./commands/classify-notes";
import * as computeVersion from "./commands/compute-version";
import * as createReleaseBranch from "./commands/create-release-branch";
import * as publishBackend from "./commands/publish-backend";
import * as setDependencyVersion from "./commands/set-dependency-version";
import * as setManifestVersion from "./commands/set-manifest-version";
import * as stampPackage from "./commands/stamp-package";
import * as tagRelease from "./commands/tag-release";
import * as updateSpmChecksum from "./commands/update-spm-checksum";
import { getRepoRoot } from "./lib/git";

await yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .option("repoRoot", {
    type: "string",
    default: getRepoRoot(),
    describe: "Repository root directory",
  })
  .command(setManifestVersion)
  .command(computeVersion)
  .command(updateSpmChecksum)
  .command(createReleaseBranch)
  .command(classifyNotes)
  .command(tagRelease)
  .command(publishBackend)
  .command(setDependencyVersion)
  .command(stampPackage)
  .demandCommand(1, "You must specify a command")
  .version(false)
  .strict()
  .help()
  .parseAsync();
