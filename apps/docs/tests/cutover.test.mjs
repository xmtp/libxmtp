import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { checkCutover, redirectTarget } from "../scripts/check-cutover.mjs";

async function site(t, origin) {
  const siteRoot = await mkdtemp(join(tmpdir(), "docs-cutover-"));
  t.after(() => rm(siteRoot, { recursive: true, force: true }));
  await mkdir(join(siteRoot, "signatures"));
  await writeFile(
    join(siteRoot, "index.html"),
    '<link rel="canonical" href="' +
      origin +
      '/"><meta property="og:image" content="' +
      origin +
      '/image.jpeg">',
  );
  await writeFile(
    join(siteRoot, "sitemap.xml"),
    "<urlset><url><loc>" + origin + "/sdk/client/</loc></url></urlset>",
  );
  await writeFile(
    join(siteRoot, "llms.txt"),
    "- [SDK](" + origin + "/sdk/client/)",
  );
  await writeFile(join(siteRoot, "llms-full.txt"), "developer guide");
  await writeFile(join(siteRoot, "llms-specs.txt"), "specifications");
  await writeFile(
    join(siteRoot, "signatures/index.html"),
    '<meta http-equiv="refresh" content="0;url=/sdk/use-signatures/">',
  );
  return { siteRoot, origin };
}

test("old-host redirects retain queries and use explicit route mappings", () => {
  const map = {
    "/signatures": "/sdk/use-signatures/",
    "/chat-apps/core-messaging/create-a-client": "/sdk/client/",
  };
  assert.equal(
    redirectTarget(
      "https://docs.xmtp.org/chat-apps/core-messaging/create-a-client/?source=old",
      map,
    ),
    "https://xmtp.org/sdk/client/?source=old",
  );
  assert.equal(
    redirectTarget("https://self-hosted-docs.xmtp.to/signatures?lang=en", map),
    "https://xmtp.org/sdk/use-signatures/?lang=en",
  );
  assert.equal(
    redirectTarget("https://docs.xmtp.org/sdk/groups/?a=1&b=2", map),
    "https://xmtp.org/sdk/groups/?a=1&b=2",
  );
  assert.throws(
    () => redirectTarget("https://other.example/sdk/", map),
    /Unknown/,
  );
  assert.throws(
    () =>
      redirectTarget("https://docs.xmtp.org/signatures", {
        "/signatures": "//other.example/",
      }),
    /leaves/,
  );
});

test("current and target origins pass complete artifact checks", async (t) => {
  await checkCutover(await site(t, "https://self-hosted-docs.xmtp.to"));
  await checkCutover(await site(t, "https://xmtp.org"));
});

test("mixed origins and missing exports cannot pass staging checks", async (t) => {
  const options = await site(t, "https://xmtp.org");
  await assert.rejects(
    checkCutover({ ...options, origin: "https://self-hosted-docs.xmtp.to" }),
    /wrong origin/,
  );
  await writeFile(
    join(options.siteRoot, "sitemap.xml"),
    "<loc>https://self-hosted-docs.xmtp.to/sdk/client/</loc>",
  );
  await assert.rejects(checkCutover(options), /Sitemap/);
  await writeFile(
    join(options.siteRoot, "sitemap.xml"),
    "<loc>https://xmtp.org/sdk/client/</loc>",
  );
  await writeFile(
    join(options.siteRoot, "llms.txt"),
    "https://self-hosted-docs.xmtp.to/sdk/client/",
  );
  await assert.rejects(checkCutover(options), /LLM/);
  await writeFile(
    join(options.siteRoot, "llms.txt"),
    "https://xmtp.org/sdk/client/",
  );
  await writeFile(join(options.siteRoot, "llms-full.txt"), "");
  await assert.rejects(checkCutover(options), /empty/);
});
