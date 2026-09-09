import { readFile, realpath } from "node:fs/promises";
import { resolve, sep } from "node:path";
import ts from "typescript";

export const examplesRoot = resolve(import.meta.dirname, "../examples");

/** Keep examples subject to the checks which the docs build promises. */
export function validateExample(source) {
  if (
    /\/\/\s*@(?:ts-(?:ignore|nocheck|expect-error)|noErrors(?:Cutted)?|noErrorValidation|noCheck)\b/u.test(
      source,
    )
  )
    throw new Error("Examples must not suppress type errors");
  const file = ts.createSourceFile(
    "example.ts",
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  function inspect(node) {
    if (node.kind === ts.SyntaxKind.AnyKeyword)
      throw new Error("Examples must not use the any type");
    ts.forEachChild(node, inspect);
  }
  inspect(file);
}

/** Read one named region and keep the full program available to Twoslash. */
export function extractRegion(source, name) {
  validateExample(source);
  const lines = source.replaceAll("\r\n", "\n").split("\n");
  const regions = new Map();
  const clean = [];
  let active;
  for (const line of lines) {
    const marker = line.match(/^\s*\/\/ #(end)?region ([\w-]+)\s*$/u);
    if (!marker) {
      clean.push(line);
      continue;
    }
    const [, end, key] = marker;
    if (!end) {
      if (active) throw new Error(`Nested region: ${key}`);
      if (regions.has(key)) throw new Error(`Duplicate region: ${key}`);
      active = { key, start: clean.length };
    } else {
      if (!active || active.key !== key)
        throw new Error(`Unmatched region end: ${key}`);
      regions.set(key, { start: active.start, end: clean.length });
      active = undefined;
    }
  }
  if (active) throw new Error(`Unclosed region: ${active.key}`);
  const region = regions.get(name);
  if (!region) throw new Error(`Missing region: ${name}`);
  const code = clean.slice(region.start, region.end).join("\n");
  if (!code.trim()) throw new Error(`Empty region: ${name}`);
  return {
    code,
    program: [
      ...clean.slice(0, region.start),
      "// ---cut-before---",
      code,
      "// ---cut-after---",
      ...clean.slice(region.end),
    ].join("\n"),
  };
}

export async function readRegion(filename, region, root = examplesRoot) {
  const absoluteRoot = await realpath(root);
  const path = await realpath(resolve(absoluteRoot, filename));
  if (!path.startsWith(`${absoluteRoot}${sep}`) || !path.endsWith(".ts"))
    throw new Error("Example source must be a TypeScript file inside examples");
  return extractRegion(await readFile(path, "utf8"), region);
}

/** Expand source fences before the Twoslash plugin checks and renders them. */
export function exampleRegions({ root = examplesRoot } = {}) {
  return {
    name: "xmtp-example-regions",
    hooks: {
      preprocessMetadata({ codeBlock }) {
        const isTypeScript = ["ts", "tsx", "typescript"].includes(
          codeBlock.language,
        );
        const isGeneratedReference =
          /\/reference\/(?:node|browser|agent)-sdk\//u.test(
            codeBlock.parentDocument?.sourceFilePath ?? "",
          );
        if (codeBlock.metaOptions.list(["source", "region"]).length) {
          codeBlock.meta += " twoslash";
        } else if (
          isTypeScript &&
          !isGeneratedReference &&
          !codeBlock.metaOptions.getBoolean("twoslash")
        ) {
          throw new Error(
            "Authored TypeScript fences need source regions or twoslash",
          );
        }
        if (codeBlock.language === "typescript") codeBlock.language = "ts";
      },
      async preprocessCode({ codeBlock }) {
        const sources = codeBlock.metaOptions.getStrings("source");
        const regions = codeBlock.metaOptions.getStrings("region");
        if (!codeBlock.metaOptions.list(["source", "region"]).length) {
          if (codeBlock.metaOptions.getBoolean("twoslash"))
            validateExample(codeBlock.code);
          return;
        }
        if (sources.length !== 1 || regions.length !== 1)
          throw new Error("Source fences need one source and one region");
        if (codeBlock.language !== "ts" || codeBlock.code.trim())
          throw new Error("Source fences must be empty TypeScript blocks");
        const { program } = await readRegion(sources[0], regions[0], root);
        const lines = codeBlock.getLines().map((_, index) => index);
        if (lines.length) codeBlock.deleteLines(lines);
        codeBlock.insertLines(0, program.split("\n"));
      },
    },
  };
}
