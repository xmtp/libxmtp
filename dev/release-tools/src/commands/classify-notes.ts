import path from "node:path";
import fs from "node:fs";
import type { ArgumentsCamelCase, Argv } from "yargs";
import { classifyNoteFiles } from "../lib/classify-notes.js";

export const command = "classify-notes";
export const describe =
  "Find and classify release note files as empty scaffolds or having content";

export function builder(yargs: Argv) {
  return yargs
    .option("releaseVersion", {
      type: "string",
      demandOption: true,
      describe: "Release version to look up (e.g. 1.0.0)",
    })
    .option("repoRoot", {
      type: "string",
      default: process.cwd(),
      describe: "Repository root directory",
    });
}

export async function handler(
  argv: ArgumentsCamelCase<{ releaseVersion: string; repoRoot: string }>,
) {
  const repoRoot = path.resolve(argv.repoRoot);
  const notesDir = path.join(repoRoot, "docs/release-notes");

  const files: { path: string; content: string }[] = [];

  if (fs.existsSync(notesDir)) {
    for (const sdk of fs.readdirSync(notesDir, { withFileTypes: true })) {
      if (!sdk.isDirectory()) continue;
      const filePath = path.join(
        "docs/release-notes",
        sdk.name,
        `${argv.releaseVersion}.md`,
      );
      const fullPath = path.join(repoRoot, filePath);
      if (fs.existsSync(fullPath)) {
        const content = fs.readFileSync(fullPath, "utf-8");
        files.push({ path: filePath, content });
      }
    }
  }

  const result = classifyNoteFiles(files);

  const output = {
    empty: result.empty,
    content: result.content,
    hasEmpty: result.empty.length > 0,
    hasContent: result.content.length > 0,
  };

  console.log(JSON.stringify(output, null, 2));
}
