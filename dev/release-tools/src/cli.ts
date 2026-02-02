#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as findLastVersion from "./commands/find-last-version.js";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .command(findLastVersion)
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
