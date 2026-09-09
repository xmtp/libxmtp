import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { compose, installLlmsFull, validateDocC } from "../scripts/compose.mjs";

test("compose adds references, redirect stubs, and validates DocC CSS", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-compose-test-"));
  const dist = join(root, "dist");
  const swift = join(root, "swift");
  const site = join(root, "site");
  await mkdir(dist);
  await mkdir(join(swift, "css"), { recursive: true });
  await writeFile(join(dist, "index.html"), "home");
  await mkdir(join(dist, "_llms-txt"), { recursive: true });
  await writeFile(join(dist, "_llms-txt/developer-guide.txt"), "# guide");
  await writeFile(
    join(dist, "llms.txt"),
    "- [Developer guide](/_llms-txt/developer-guide.txt)",
  );
  await writeFile(
    join(swift, "index.html"),
    '<script>baseUrl = "/reference/swift/"</script><link href="css/site.css">',
  );
  await writeFile(join(swift, "css/site.css"), "body{}");
  await writeFile(
    join(root, "redirects.json"),
    JSON.stringify({ "/before/": "/after/" }),
  );
  await mkdir(join(site, "reference"), { recursive: true });
  await compose({
    distRoot: dist,
    siteRoot: site,
    redirectsPath: join(root, "redirects.json"),
  });
  await mkdir(join(site, "reference/swift"), { recursive: true });
  await writeFile(
    join(site, "reference/swift/index.html"),
    '<script>baseUrl = "/reference/swift/"</script><link href="css/site.css">',
  );
  await mkdir(join(site, "reference/swift/css"), { recursive: true });
  await writeFile(join(site, "reference/swift/css/site.css"), "body{}");
  assert.equal(await readFile(join(site, "index.html"), "utf8"), "home");
  assert.match(
    await readFile(join(site, "before/index.html"), "utf8"),
    /url=\/after\//,
  );
  assert.match(
    await readFile(join(site, "before/index.html"), "utf8"),
    /name="robots" content="noindex"/,
  );
  assert.match(await validateDocC(site), /site\.css$/);
  assert.equal(await readFile(join(site, "llms-full.txt"), "utf8"), "# guide");
  assert.match(
    await readFile(join(site, "llms.txt"), "utf8"),
    /\/llms-full\.txt/,
  );
});

test("llms install rejects an index that does not expose the full set", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-llms-test-"));
  await mkdir(join(root, "_llms-txt"));
  await writeFile(join(root, "_llms-txt/developer-guide.txt"), "# guide");
  await writeFile(join(root, "llms.txt"), "- [Small set](/small.txt)");
  await assert.rejects(installLlmsFull(root), /does not link/);
});

test("compose is repeatable and removes stale output", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-compose-repeat-test-"));
  const dist = join(root, "dist");
  const site = join(root, "site");
  await mkdir(join(dist, "_llms-txt"), { recursive: true });
  await writeFile(join(dist, "index.html"), "home");
  await writeFile(join(dist, "_llms-txt/developer-guide.txt"), "# guide");
  await writeFile(
    join(dist, "llms.txt"),
    "- [Developer guide](/_llms-txt/developer-guide.txt)",
  );
  await writeFile(
    join(root, "redirects.json"),
    JSON.stringify({ "/old/": "/new/" }),
  );
  const options = {
    distRoot: dist,
    siteRoot: site,
    redirectsPath: join(root, "redirects.json"),
  };
  await compose(options);
  await writeFile(join(site, "stale.html"), "stale");
  await compose(options);
  await assert.rejects(readFile(join(site, "stale.html")), /ENOENT/);
  assert.match(await readFile(join(site, "old/index.html"), "utf8"), /Moved/);
});

test("compose preserves the prior output when stage validation fails", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-compose-failure-test-"));
  const dist = join(root, "dist");
  const site = join(root, "site");
  await mkdir(dist);
  await mkdir(site);
  await writeFile(join(site, "index.html"), "known good");
  await writeFile(join(dist, "index.html"), "new but invalid");
  await writeFile(join(root, "redirects.json"), "{}");
  await assert.rejects(
    compose({
      distRoot: dist,
      siteRoot: site,
      redirectsPath: join(root, "redirects.json"),
    }),
  );
  assert.equal(await readFile(join(site, "index.html"), "utf8"), "known good");
});

test("compose rejects nested source and output directories", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-compose-path-test-"));
  await assert.rejects(
    compose({
      distRoot: root,
      siteRoot: join(root, "site"),
      redirectsPath: join(root, "redirects.json"),
    }),
    /must be disjoint/,
  );
});
