import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { walkFiles } from "./validation-lib.mjs";

const OLD_HOSTS = new Set(["docs.xmtp.org", "self-hosted-docs.xmtp.to"]);
const DOCS_HOSTS = new Set([...OLD_HOSTS, "xmtp.org"]);

export function redirectTarget(request, redirects) {
  const source = new URL(request);
  if (!OLD_HOSTS.has(source.hostname)) throw new Error("Unknown redirect host");
  const key = source.pathname.replace(/\/+$/, "") || "/";
  const path = redirects[key] ?? source.pathname;
  const target = new URL(path, "https://xmtp.org");
  if (target.origin !== "https://xmtp.org")
    throw new Error("Redirect leaves the target origin");
  target.search = source.search;
  return target.href;
}

function assertOrigin(value, origin, label) {
  if (new URL(value).origin !== origin)
    throw new Error(label + " has the wrong origin");
}

export async function checkCutover({ siteRoot, origin }) {
  const expected = new URL(origin);
  if (
    expected.protocol !== "https:" ||
    expected.href !== expected.origin + "/"
  ) {
    throw new Error("Expected an HTTPS origin without a path");
  }
  const root = await readFile(join(siteRoot, "index.html"), "utf8");
  const canonical = root.match(
    /<link[^>]*rel=["']canonical["'][^>]*href=["']([^"']+)["']/i,
  );
  if (!canonical) throw new Error("Homepage canonical is missing");
  assertOrigin(canonical[1], expected.origin, "Homepage canonical");
  const image = root.match(
    /<meta[^>]*property=["']og:image["'][^>]*content=["']([^"']+)["']/i,
  );
  if (!image) throw new Error("Homepage social image is missing");
  assertOrigin(image[1], expected.origin, "Social image");
  let sitemaps = 0;
  for (const file of await walkFiles(siteRoot)) {
    if (!file.endsWith(".xml") || !file.includes("sitemap")) continue;
    sitemaps++;
    const text = await readFile(file, "utf8");
    for (const match of text.matchAll(/<loc>([^<]+)<\/loc>/g)) {
      assertOrigin(
        match[1].replaceAll("&amp;", "&"),
        expected.origin,
        "Sitemap",
      );
    }
  }
  if (!sitemaps) throw new Error("Sitemap is missing");
  const index = await readFile(join(siteRoot, "llms.txt"), "utf8");
  for (const match of index.matchAll(/https?:\/\/[^\s)<>]+/g)) {
    const url = new URL(match[0]);
    if (DOCS_HOSTS.has(url.hostname))
      assertOrigin(url.href, expected.origin, "LLM index");
  }
  for (const file of ["llms-full.txt", "llms-specs.txt"]) {
    if (!(await readFile(join(siteRoot, file), "utf8")).trim())
      throw new Error(file + " is empty");
  }
  const signatures = await readFile(
    join(siteRoot, "signatures/index.html"),
    "utf8",
  );
  if (!signatures.includes("url=/sdk/use-signatures/"))
    throw new Error("Signature route has the wrong destination");
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await checkCutover({
    siteRoot: resolve(process.argv[3] ?? "_site"),
    origin: process.argv[2] ?? "https://self-hosted-docs.xmtp.to",
  });
  console.log("OK: site origin, exports, and signature route");
}
