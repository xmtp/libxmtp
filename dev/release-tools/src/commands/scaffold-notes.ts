import path from "node:path";
import fs from "node:fs";
import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { getCommitsBetween } from "../lib/git.js";
import { findLastVersion } from "./find-last-version.js";

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

  let commitSection: string;
  if (sinceTag) {
    const commits = getCommitsBetween(repoRoot, sinceTag, "HEAD");
    commitSection =
      commits.length > 0
        ? commits.map((c) => `- ${c}`).join("\n")
        : "- No changes since last release";
  } else {
    commitSection =
      "This is the first release from the monorepo. No previous release tag was found to compare against.";
  }

  const content = `# ${config.name} SDK ${version}

## Highlights

<!-- AI-generated draft - please review and edit -->

## What's Changed

${commitSection}

## Breaking Changes

<!-- List any breaking changes here -->

## New Features

<!-- List new features here -->

## Bug Fixes

<!-- List bug fixes here -->

## Dependencies

<!-- Note any dependency changes -->
`;

  fs.writeFileSync(outputPath, content);
  return outputPath;
}

export const command = "scaffold-notes";
export const describe = "Generate a release notes template from git history";

export function builder(yargs: Argv) {
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
  argv: ArgumentsCamelCase<{ sdk: string; since?: string }>,
) {
  const sinceTag =
    argv.since ??
    (() => {
      const lastVersion = findLastVersion(argv.sdk, process.cwd());
      if (!lastVersion) return null;
      const config = getSdkConfig(argv.sdk);
      return `${config.tagPrefix}${lastVersion}`;
    })();

  const outputPath = scaffoldNotes(argv.sdk, process.cwd(), sinceTag);
  console.log(outputPath);
}
