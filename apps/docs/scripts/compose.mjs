import {
  cp,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { installReferences } from "./references.mjs";

export async function copyTree(source, destination) {
  await stat(source);
  await mkdir(dirname(destination), { recursive: true });
  await cp(source, destination, { recursive: true, force: true });
}

function assertDisjointDirectories(first, second) {
  const a = resolve(first);
  const b = resolve(second);
  const aToB = relative(a, b);
  const bToA = relative(b, a);
  if (
    a === b ||
    (!aToB.startsWith("..") && aToB !== "") ||
    (!bToA.startsWith("..") && bToA !== "")
  ) {
    throw new Error(`compose directories must be disjoint: ${a} and ${b}`);
  }
  if (a === dirname(a) || b === dirname(b)) {
    throw new Error("compose cannot use a filesystem root");
  }
}

export async function validateDocC(siteRoot) {
  const indexPath = join(siteRoot, "reference/swift/index.html");
  const html = await readFile(indexPath, "utf8");
  if (
    !html.includes('baseUrl = "/reference/swift/"') &&
    !html.includes("baseUrl = '/reference/swift/'")
  ) {
    throw new Error("Swift DocC has an incorrect base URL");
  }
  const match = html.match(/(?:href|src)=["']([^"']+\.css(?:\?[^"']*)?)["']/i);
  if (!match) throw new Error("Swift DocC does not load a CSS asset");
  const asset = match[1].split("?", 1)[0];
  const assetPath = asset.startsWith("/")
    ? join(siteRoot, asset)
    : resolve(dirname(indexPath), asset);
  await stat(assetPath);
  return assetPath;
}

export async function writeRedirects(siteRoot, redirects) {
  for (const [from, to] of Object.entries(redirects)) {
    const path = resolve(siteRoot, from.replace(/^\//, ""), "index.html");
    if (!path.startsWith(`${resolve(siteRoot)}/`))
      throw new Error(`redirect path escapes the site: ${from}`);
    try {
      await stat(path);
      throw new Error(`redirect would overwrite a page: ${from}`);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    await mkdir(dirname(path), { recursive: true });
    const escaped = to.replaceAll("&", "&amp;").replaceAll('"', "&quot;");
    await writeFile(
      path,
      `<!doctype html><meta charset="utf-8"><meta name="robots" content="noindex"><meta http-equiv="refresh" content="0;url=${escaped}"><link rel="canonical" href="${escaped}"><title>Moved</title><a href="${escaped}">Continue</a>\n`,
    );
  }
}

export async function installLlmsExports(siteRoot) {
  const indexPath = join(siteRoot, "llms.txt");
  let index = await readFile(indexPath, "utf8");
  for (const [source, target] of [
    ["_llms-txt/developer-guide.txt", "llms-full.txt"],
    ["_llms-txt/specs.txt", "llms-specs.txt"],
  ]) {
    if (!index.includes(`/${source}`)) {
      throw new Error(`llms.txt does not link to ${source}`);
    }
    await cp(join(siteRoot, source), join(siteRoot, target), { force: true });
    index = index.replaceAll(`/${source}`, `/${target}`);
  }
  // The plugin's default full set is replaced by the focused developer guide.
  index = index
    .replace(/^- \[Complete documentation\].*\n?/gm, "")
    .replace(
      "- The complete documentation includes all content from the official documentation",
      "- Start with the developer guide. Load the specs separately for protocol requirements.",
    );
  await writeFile(indexPath, index);
}

export async function compose({
  distRoot,
  siteRoot,
  redirectsPath,
  installReferences,
}) {
  const output = resolve(siteRoot);
  const source = resolve(distRoot);
  assertDisjointDirectories(source, output);
  await mkdir(dirname(output), { recursive: true });
  const stage = await mkdtemp(
    join(dirname(output), `.${basename(output)}-stage-`),
  );
  let backup;
  try {
    await copyTree(source, stage);
    await installLlmsExports(stage);
    if (installReferences) await installReferences({ siteRoot: stage });
    const redirects = JSON.parse(await readFile(redirectsPath, "utf8"));
    await writeRedirects(stage, redirects);
    // The Swift reference is built on push only; see references.mjs.
    if (installReferences && process.env.DOCS_SKIP_NATIVE_REFERENCES !== "1")
      await validateDocC(stage);

    try {
      await stat(output);
      backup = await mkdtemp(
        join(dirname(output), `.${basename(output)}-backup-`),
      );
      await rm(backup, { recursive: true });
      await rename(output, backup);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    try {
      await rename(stage, output);
    } catch (error) {
      if (backup) await rename(backup, output);
      throw error;
    }
    if (backup) await rm(backup, { recursive: true });
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  const docsRoot = resolve(import.meta.dirname, "..");
  await compose({
    distRoot: process.env.DIST_ROOT ?? join(docsRoot, "dist"),
    siteRoot: process.env.SITE_ROOT ?? join(docsRoot, "_site"),
    redirectsPath:
      process.env.REDIRECTS ?? join(docsRoot, "parity/redirects.json"),
    installReferences: ({ siteRoot }) => installReferences({ siteRoot }),
  });
  console.log("OK: composed documentation site");
}
