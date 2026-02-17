import path from "node:path";
import fs from "node:fs";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { GlobalArgs } from "../types";
import { Sdk } from "../types";
import { classifyNoteFiles } from "../lib/classify-notes";
import { SDK_CONFIGS } from "../lib/sdk-config";
import { listTags } from "../lib/git";

export const command = "classify-notes";
export const describe =
  "Find and classify release note files as empty scaffolds or having content";

export function builder(yargs: Argv<GlobalArgs>) {
  return yargs;
}

export async function handler(argv: ArgumentsCamelCase<GlobalArgs>) {
  const repoRoot = path.resolve(argv.repoRoot);
  const tags = new Set(listTags(repoRoot));

  const files: { path: string; content: string }[] = [];
  const skipped: { sdk: string; version: string; reason: string }[] = [];

  for (const sdk of Object.values(Sdk)) {
    const config = SDK_CONFIGS[sdk];

    let version: string;
    try {
      version = config.manifest.readVersion(repoRoot);
    } catch {
      skipped.push({
        sdk,
        version: "unknown",
        reason: "manifest not found or unreadable",
      });
      continue;
    }

    const expectedTag = `${config.tagPrefix}${version}`;
    if (tags.has(expectedTag)) {
      skipped.push({
        sdk,
        version,
        reason: `tag ${expectedTag} already exists`,
      });
      continue;
    }

    // Derive the directory name: strip trailing dash from tagPrefix
    const sdkName = config.tagPrefix.replace(/-$/, "") || config.tagPrefix;
    const filePath = path.join("docs/release-notes", sdkName, `${version}.md`);
    const fullPath = path.join(repoRoot, filePath);

    if (fs.existsSync(fullPath)) {
      try {
        const content = fs.readFileSync(fullPath, "utf-8");
        files.push({ path: filePath, content });
      } catch {
        skipped.push({ sdk, version, reason: "file unreadable" });
      }
    }
  }

  const result = classifyNoteFiles(files);

  const output = {
    empty: result.empty,
    content: result.content,
    hasEmpty: result.empty.length > 0,
    hasContent: result.content.length > 0,
    skipped,
  };

  console.log(JSON.stringify(output, null, 2));
}
