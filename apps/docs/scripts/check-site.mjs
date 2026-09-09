import { readFile, stat } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { readRegion } from "./example-regions.mjs";
import {
  contentRoute,
  firstExisting,
  isLocalLink,
  linksFromMarkdown,
  outputCandidates,
  resolveMarkdownLink,
  walkFiles,
} from "./validation-lib.mjs";

const SOURCE_EXTENSIONS = new Set([".md", ".mdx"]);
const ALLOWED_SCHEMES = /^(?:https?:|mailto:|tel:|data:|javascript:)/i;
const NATIVE_ENTRYPOINTS = [
  "rust/index.html",
  "rust/xmtp_mls/index.html",
  "reference/kotlin/index.html",
  "reference/swift/index.html",
];

export async function checkSource({ contentRoot }) {
  const failures = [];
  const files = (await walkFiles(contentRoot)).filter((file) =>
    SOURCE_EXTENSIONS.has(file.slice(file.lastIndexOf("."))),
  );
  for (const file of files) {
    const source = await readFile(file, "utf8");
    const label = relative(contentRoot, file);
    if (source.includes(":::code-group"))
      failures.push(`${label}: literal :::code-group is not allowed`);
    for (const group of source.matchAll(
      /<Tabs\s+syncKey=["']sdk["'][^>]*>([\s\S]*?)<\/Tabs>/gu,
    )) {
      const labels = [
        ...group[1].matchAll(/<TabItem\s+label=["']([^"']+)["']/gu),
      ].map((match) => match[1]);
      if (labels.join(",") !== "Browser,Node,Kotlin,Swift") {
        failures.push(
          `${label}: SDK tabs must be Browser, Node, Kotlin, Swift in that order`,
        );
      }
    }
    for (const link of linksFromMarkdown(source)) {
      if (
        ALLOWED_SCHEMES.test(link) ||
        link.startsWith("#") ||
        link.startsWith("//") ||
        link.startsWith("{")
      )
        continue;
      const bare = link.split(/[?#]/, 1)[0];
      if (!bare) continue;
      if (bare.startsWith("/")) continue;
      if (
        isLocalLink(link) &&
        !(await firstExisting(resolveMarkdownLink(contentRoot, file, link)))
      ) {
        failures.push(`${label}: broken local link ${link}`);
      }
    }
  }
  return failures;
}

export async function checkBuilt({
  contentRoot,
  outputRoot,
  redirectsPath,
  oldUrlsPath,
  examplesRoot = resolve(contentRoot, "../../../examples"),
}) {
  const failures = [];
  for (const entrypoint of NATIVE_ENTRYPOINTS) {
    const entryPath = join(outputRoot, entrypoint);
    let html;
    try {
      html = await readFile(entryPath, "utf8");
    } catch {
      failures.push(`native reference entrypoint is missing: ${entrypoint}`);
      continue;
    }
    for (const match of html.matchAll(
      /(?:href|src)=["']([^"'?#]+\.(?:css|js|mjs|png|jpe?g|svg|woff2?))[^"']*["']/giu,
    )) {
      if (/^(?:https?:)?\/\//iu.test(match[1])) continue;
      const assetPath = match[1].startsWith("/")
        ? join(outputRoot, match[1])
        : resolve(entryPath, "..", match[1]);
      try {
        await stat(assetPath);
      } catch {
        failures.push(
          `${entrypoint}: referenced asset is missing: ${match[1]}`,
        );
      }
    }
  }
  const files = (await walkFiles(contentRoot)).filter((file) =>
    /\.mdx?$/.test(file),
  );
  const routes = new Set(files.map((file) => contentRoute(contentRoot, file)));
  const redirects = JSON.parse(await readFile(redirectsPath, "utf8"));
  const known = new Set([...routes, ...Object.keys(redirects)]);
  for (const [from, to] of Object.entries(redirects)) {
    if (!from.startsWith("/") || !to.startsWith("/"))
      failures.push(`redirect must use root-relative routes: ${from} -> ${to}`);
    const route = to.endsWith("/") ? to : `${to}/`;
    if (
      !known.has(to) &&
      !routes.has(route) &&
      !(await firstExisting(outputCandidates(outputRoot, route)))
    ) {
      failures.push(
        `redirect target is not a built page or redirect: ${from} -> ${to}`,
      );
    }
  }
  for (const from of Object.keys(redirects)) {
    const seen = new Set([from]);
    let target = redirects[from];
    while (redirects[target]) {
      if (seen.has(target)) {
        failures.push(`redirect cycle includes ${target}`);
        break;
      }
      seen.add(target);
      target = redirects[target];
    }
  }
  for (const route of routes)
    if (!(await firstExisting(outputCandidates(outputRoot, route))))
      failures.push(`built page is missing: ${route}`);
  const specsRoot = join(outputRoot, "specs");
  try {
    for (const file of (await walkFiles(specsRoot)).filter((path) =>
      path.endsWith(".html"),
    )) {
      const html = await readFile(file, "utf8");
      if (
        /id=["']review-(?:record|log)["']/iu.test(html) ||
        /<h[1-6][^>]*>\s*Review (?:record|log)\s*<\/h[1-6]>/iu.test(html)
      ) {
        failures.push(
          `${relative(outputRoot, file)}: built spec exposes a review record or log`,
        );
      }
    }
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  for (const file of files) {
    const source = await readFile(file, "utf8");
    for (const link of linksFromMarkdown(source).filter((value) =>
      value.startsWith("/"),
    )) {
      const [path, anchor] = link.split("#", 2);
      const route = path.endsWith("/") ? path : `${path}/`;
      let pagePath = await firstExisting(outputCandidates(outputRoot, route));
      if (!pagePath && redirects[path])
        pagePath = await firstExisting(
          outputCandidates(outputRoot, redirects[path]),
        );
      if (!pagePath) {
        failures.push(
          `${relative(contentRoot, file)}: built link target is missing: ${link}`,
        );
        continue;
      }
      if (anchor) {
        const html = await readFile(pagePath, "utf8");
        if (
          !html.includes(`id="${anchor}"`) &&
          !html.includes(`id='${anchor}'`)
        ) {
          failures.push(
            `${relative(contentRoot, file)}: built anchor is missing: ${link}`,
          );
        }
      }
    }
  }
  if (oldUrlsPath) {
    const oldUrls = (await readFile(oldUrlsPath, "utf8"))
      .split(/\r?\n/u)
      .map((line) => line.trim())
      .filter(Boolean);
    for (const oldUrl of oldUrls) {
      const route = oldUrl.endsWith("/") ? oldUrl : `${oldUrl}/`;
      if (
        !(oldUrl in redirects) &&
        !(route in redirects) &&
        !(await firstExisting(outputCandidates(outputRoot, route)))
      ) {
        failures.push(`old URL is not a page or redirect: ${oldUrl}`);
      }
    }
  }
  const llmsFull = join(outputRoot, "llms-full.txt");
  try {
    const text = await readFile(llmsFull, "utf8");
    const bytes = (await stat(llmsFull)).size;
    const pages = (text.match(/^# /gm) ?? []).length;
    if (bytes <= 120_000 || bytes >= 900_000)
      failures.push(`llms-full.txt is ${bytes} bytes; expected 120001-899999`);
    if (pages < 35)
      failures.push(`llms-full.txt has ${pages} pages; expected at least 35`);
    for (const file of files) {
      const markdown = await readFile(file, "utf8");
      for (const fence of markdown.matchAll(
        /^```ts\s+[^\n]*\bsource=["']([^"']+)["'][^\n]*\bregion=["']([^"']+)["'][^\n]*\n[\s\S]*?^```\s*$/gmu,
      )) {
        const { code } = await readRegion(fence[1], fence[2], examplesRoot);
        const compact = (value) => value.replace(/\s+/gu, " ").trim();
        if (!compact(text).includes(compact(code))) {
          failures.push(
            `llms-full.txt is missing resolved snippet ${fence[1]}#${fence[2]}`,
          );
        }
      }
    }
  } catch {
    failures.push("llms-full.txt is missing");
  }
  try {
    await stat(join(outputRoot, "llms.txt"));
  } catch {
    failures.push("llms.txt is missing");
  }
  return failures;
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  const root = process.cwd();
  const contentRoot =
    process.env.CONTENT_ROOT ?? join(root, "src/content/docs");
  const failures = await checkSource({ contentRoot });
  if (process.argv.includes("--built"))
    failures.push(
      ...(await checkBuilt({
        contentRoot,
        outputRoot: process.env.OUTPUT_ROOT ?? join(root, "_site"),
        redirectsPath:
          process.env.REDIRECTS ?? join(root, "parity/redirects.json"),
        oldUrlsPath: process.env.OLD_URLS ?? join(root, "parity/old-urls.txt"),
      })),
    );
  if (failures.length) {
    console.error(failures.join("\n"));
    process.exitCode = 1;
  } else console.log("OK: documentation validation passed");
}
