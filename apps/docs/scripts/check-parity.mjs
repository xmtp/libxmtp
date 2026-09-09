import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { readRegion } from "./example-regions.mjs";
import { normalizeText, readParityRecords } from "./validation-lib.mjs";

export async function expandSourceRegions(markdown, examplesRoot) {
  const expression =
    /^(```ts\s+([^\n]*\bsource=["']([^"']+)["'][^\n]*\bregion=["']([^"']+)["'][^\n]*)\n)([\s\S]*?)^(```\s*)$/gmu;
  let expanded = "";
  let offset = 0;
  for (const match of markdown.matchAll(expression)) {
    if (match[5].trim())
      throw new Error(
        `Source fence ${match[3]}#${match[4]} must have an empty body`,
      );
    const { code } = await readRegion(match[3], match[4], examplesRoot);
    expanded += markdown.slice(offset, match.index);
    expanded += `${match[1]}${code}\n${match[6]}`;
    offset = match.index + match[0].length;
  }
  return expanded + markdown.slice(offset);
}

export async function checkParity({ parityRoot, contentRoot, examplesRoot }) {
  const failures = [];
  const records = await readParityRecords(parityRoot);
  for (const [index, record] of records.entries()) {
    const label = `${record.manifest}[${index}]`;
    if (!record.source || !record.target || !Array.isArray(record.blocks)) {
      failures.push(`${label}: source, target, and blocks are required`);
      continue;
    }
    const relativeTarget = record.target
      .replace(/^apps\/docs\/src\/content\/docs\//u, "")
      .replace(/^src\/content\/docs\//u, "");
    const targetPath = resolve(contentRoot, relativeTarget);
    if (!targetPath.startsWith(`${resolve(contentRoot)}/`)) {
      failures.push(`${label}: target escapes the content directory`);
      continue;
    }
    let target;
    try {
      const markdown = await readFile(targetPath, "utf8");
      target = normalizeText(
        await expandSourceRegions(
          markdown,
          examplesRoot ?? resolve(contentRoot, "../../../examples"),
        ),
      );
    } catch {
      failures.push(`${label}: target does not exist: ${record.target}`);
      continue;
    }
    for (const [blockIndex, block] of record.blocks.entries()) {
      if (typeof block !== "string" || !normalizeText(block)) {
        failures.push(`${label}: block ${blockIndex + 1} is empty`);
      } else if (!target.includes(normalizeText(block))) {
        failures.push(
          `${label}: retained block ${blockIndex + 1} changed in ${record.target}`,
        );
      }
    }
  }
  return failures;
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  const root = process.cwd();
  const failures = await checkParity({
    parityRoot: process.env.PARITY_ROOT ?? join(root, "parity"),
    contentRoot: process.env.CONTENT_ROOT ?? join(root, "src/content/docs"),
  });
  if (failures.length) {
    console.error(failures.join("\n"));
    process.exitCode = 1;
  } else console.log("OK: all frozen retained blocks are present");
}
