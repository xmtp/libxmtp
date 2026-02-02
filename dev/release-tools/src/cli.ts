#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
