#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as findLastVersion from "./commands/find-last-version.js";
import * as bumpVersion from "./commands/bump-version.js";
import * as computeVersion from "./commands/compute-version.js";
import * as updateSpmChecksum from "./commands/update-spm-checksum.js";
import * as scaffoldNotes from "./commands/scaffold-notes.js";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .command(findLastVersion)
  .command(bumpVersion)
  .command(computeVersion)
  .command(updateSpmChecksum)
  .command(scaffoldNotes)
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
