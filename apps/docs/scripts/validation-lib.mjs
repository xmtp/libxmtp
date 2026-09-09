import { readFile, readdir, stat } from "node:fs/promises";
import { extname, join, relative, resolve, sep } from "node:path";

export function assertTypeDocValidation(logger, label) {
  if (logger.hasErrors() || logger.hasWarnings()) {
    throw new Error(`${label} TypeDoc validation failed.`);
  }
}

export const normalizeText = (value) =>
  value
    .replace(/^:::[a-z]+(?:\[.*?\])?\s*$/gm, "")
    .replace(/^:::\s*$/gm, "")
    .replace(/^```.*$/gm, "```")
    .replace(/<\/?[A-Z][^>]*>/g, "")
    .replace(/\s+/g, " ")
    .trim();

export async function walkFiles(root) {
  const files = [];
  async function walk(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await walk(path);
      else files.push(path);
    }
  }
  await walk(root);
  return files.sort();
}

export function contentRoute(contentRoot, file) {
  let route = relative(contentRoot, file)
    .split(sep)
    .join("/")
    .replace(/\.(?:md|mdx)$/, "");
  route = route.replace(/(?:^|\/)index$/, "");
  return route ? `/${route.replace(/\/+$/, "")}/` : "/";
}

export function outputCandidates(outputRoot, route) {
  const clean = route.replace(/^\//, "").replace(/\/$/, "");
  return clean
    ? [join(outputRoot, clean, "index.html"), join(outputRoot, `${clean}.html`)]
    : [join(outputRoot, "index.html")];
}

export function linksFromMarkdown(source) {
  const links = [];
  const expression =
    /(?:!?\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)|\b(?:href|src)=["']([^"']+)["'])/g;
  for (const match of source.matchAll(expression))
    links.push(match[1] ?? match[2]);
  return links;
}

export function isLocalLink(link) {
  return !/^(?:[a-z]+:|#|\/\/)/i.test(link) && !link.startsWith("{");
}

export function resolveMarkdownLink(contentRoot, sourceFile, link) {
  const bare = decodeURIComponent(link.split(/[?#]/, 1)[0]);
  const path = bare.startsWith("/")
    ? resolve(contentRoot, `.${bare}`)
    : resolve(sourceFile, "..", bare);
  if (extname(path)) return [path];
  return [
    path,
    `${path}.md`,
    `${path}.mdx`,
    join(path, "index.md"),
    join(path, "index.mdx"),
  ];
}

export async function firstExisting(paths) {
  for (const path of paths) {
    try {
      await stat(path);
      return path;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  return undefined;
}

export async function readParityRecords(directory) {
  const records = [];
  for (const name of ["start.json", "sdk.json", "other.json"]) {
    const data = JSON.parse(await readFile(join(directory, name), "utf8"));
    if (!Array.isArray(data)) throw new Error(`${name} must contain an array`);
    for (const record of data) records.push({ ...record, manifest: name });
  }
  return records;
}
