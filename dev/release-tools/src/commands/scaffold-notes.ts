import path from "node:path";
import fs from "node:fs";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs } from "../types";
import { getSdkConfig } from "../lib/sdk-config";
import { findLastVersion } from "./find-last-version";

export function scaffoldNotes(
  sdk: string,
  repoRoot: string,
  sinceTag: string | null,
): string {
  const config = getSdkConfig(sdk);
  const version = config.manifest.readVersion(repoRoot);

  const sdkName = config.tagPrefix.replace(/-$/, "");
  const notesDir = path.join(repoRoot, "docs/release-notes", sdkName);
  fs.mkdirSync(notesDir, { recursive: true });
  const outputPath = path.join(notesDir, `${version}.md`);

  const previousTag = sinceTag ?? "null";

  const content = `---
sdk: ${sdk}
previous_release_tag: ${previousTag}
---

# ${config.name} SDK ${version}

## Highlights

<!-- AI-generated draft - please review and edit -->

## What's Changed

<!-- Describe what changed in this release -->

## Breaking Changes

<!-- List any breaking changes here -->

## New Features

<!-- List new features here -->

## Bug Fixes

<!-- List bug fixes here -->
`;

  fs.writeFileSync(outputPath, content);
  return outputPath;
}

export const command = "scaffold-notes";
export const describe = "Generate a release notes template from git history";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("since", {
      type: "string",
      describe: "Tag to diff from (defaults to last stable release tag)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<GlobalArgs & { sdk: string; since?: string }>,
) {
  const sinceTag =
    argv.since ??
    (() => {
      const lastVersion = findLastVersion(argv.sdk, argv.repoRoot);
      if (!lastVersion) return null;
      const config = getSdkConfig(argv.sdk);
      return `${config.tagPrefix}${lastVersion}`;
    })();

  const outputPath = scaffoldNotes(argv.sdk, argv.repoRoot, sinceTag);
  console.log(outputPath);
}
