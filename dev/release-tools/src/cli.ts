#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as bumpVersion from "./commands/bump-version";
import * as setManifestVersion from "./commands/set-manifest-version";
import * as computeVersion from "./commands/compute-version";
import * as updateSpmChecksum from "./commands/update-spm-checksum";
import * as createReleaseBranch from "./commands/create-release-branch";
import * as classifyNotes from "./commands/classify-notes";
import * as tagRelease from "./commands/tag-release";
import * as setDevcontainerImage from "./commands/set-devcontainer-image";
import * as pendingVersion from "./commands/pending-version";
import * as resolveSdkVersion from "./commands/resolve-sdk-version";
import * as listSdks from "./commands/list-sdks";
import { getRepoRoot } from "./lib/git";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .option("repoRoot", {
    type: "string",
    default: getRepoRoot(),
    describe: "Repository root directory",
  })
  .command(bumpVersion)
  .command(setManifestVersion)
  .command(computeVersion)
  .command(updateSpmChecksum)
  .command(createReleaseBranch)
  .command(classifyNotes)
  .command(tagRelease)
  .command(setDevcontainerImage)
  .command(pendingVersion)
  .command(resolveSdkVersion)
  .command(listSdks)
  .demandCommand(1, "You must specify a command")
  .version(false)
  .strict()
  .help()
  .parse();
