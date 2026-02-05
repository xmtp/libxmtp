#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as findLastVersion from "./commands/find-last-version.js";
import * as bumpVersion from "./commands/bump-version.js";
import * as setManifestVersion from "./commands/set-manifest-version.js";
import * as computeVersion from "./commands/compute-version.js";
import * as updateSpmChecksum from "./commands/update-spm-checksum.js";
import * as scaffoldNotes from "./commands/scaffold-notes.js";
import * as createReleaseBranch from "./commands/create-release-branch.js";
import * as classifyNotes from "./commands/classify-notes.js";
import { getRepoRoot } from "./lib/git.js";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .option("repoRoot", {
    type: "string",
    default: getRepoRoot(),
    describe: "Repository root directory",
  })
  .command(findLastVersion)
  .command(bumpVersion)
  .command(setManifestVersion)
  .command(computeVersion)
  .command(updateSpmChecksum)
  .command(scaffoldNotes)
  .command(createReleaseBranch)
  .command(classifyNotes)
  .demandCommand(1, "You must specify a command")
  .version(false)
  .strict()
  .help()
  .parse();
