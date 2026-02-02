#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as findLastVersion from "./commands/find-last-version.js";
import * as bumpVersion from "./commands/bump-version.js";
import * as computeVersion from "./commands/compute-version.js";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .command(findLastVersion)
  .command(bumpVersion)
  .command(computeVersion)
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
