import path from "node:path";
import fs from "node:fs";
import { stringify } from "smol-toml";
import { getSdkConfig } from "../lib/sdk-config";

export function scaffoldNotes(
  sdk: string,
  repoRoot: string,
  previousVersion: string,
  sinceTag: string | null,
): string {
  const config = getSdkConfig(sdk);
  const version = config.manifest.readVersion(repoRoot);

  const sdkName = config.tagPrefix.replace(/-$/, "");
  const notesDir = path.join(repoRoot, "docs/release-notes", sdkName);
  fs.mkdirSync(notesDir, { recursive: true });
  const outputPath = path.join(notesDir, `${version}.md`);

  const frontmatter: Record<string, string> = { sdk };
  frontmatter.previous_release_version = previousVersion;
  if (sinceTag !== null) {
    frontmatter.previous_release_tag = sinceTag;
  }
  const toml = stringify(frontmatter).trimEnd();

  const content = `---
${toml}
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
