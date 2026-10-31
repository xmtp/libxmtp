import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { checkBuilt, checkSource } from "../scripts/check-site.mjs";
import {
  assertTypeDocValidation,
  contentRoute,
} from "../scripts/validation-lib.mjs";

async function fixture() {
  return mkdtemp(join(tmpdir(), "xmtp-docs-test-"));
}

test("TypeDoc validation rejects warnings and errors", () => {
  assert.doesNotThrow(() =>
    assertTypeDocValidation(
      { hasErrors: () => false, hasWarnings: () => false },
      "Node SDK",
    ),
  );
  for (const logger of [
    { hasErrors: () => false, hasWarnings: () => true },
    { hasErrors: () => true, hasWarnings: () => false },
  ]) {
    assert.throws(
      () => assertTypeDocValidation(logger, "Node SDK"),
      /Node SDK TypeDoc validation failed/,
    );
  }
});

test("content routes support index pages", () => {
  assert.equal(contentRoute("/docs", "/docs/sdk/index.mdx"), "/sdk/");
  assert.equal(contentRoute("/docs", "/docs/quickstart.md"), "/quickstart/");
});

test("TypeDoc source routes follow Astro slugs without changing other routes", () => {
  for (const sdk of ["node", "browser", "agent", "react-native"]) {
    assert.equal(
      contentRoute("/docs", `/docs/reference/${sdk}-sdk/classes/Client.md`),
      `/reference/${sdk}-sdk/classes/client/`,
    );
  }
  assert.equal(
    contentRoute("/docs", "/docs/reference/agent-sdk/README.md"),
    "/reference/agent-sdk/readme/",
  );
  assert.equal(
    contentRoute("/docs", "/docs/reference/node-sdk/Some Group/Index.md"),
    "/reference/node-sdk/some-group/",
  );
  assert.equal(contentRoute("/docs", "/docs/guide/Case.md"), "/guide/Case/");
  assert.equal(
    contentRoute("/docs", "/docs/reference/kotlin/Client.md"),
    "/reference/kotlin/Client/",
  );
});

test("TypeDoc canonical pages do not hide missing pages, anchors or case errors", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  const source = join(content, "reference/node-sdk/classes/Client.md");
  const page = join(output, "reference/node-sdk/classes/client/index.html");
  await mkdir(join(content, "reference/node-sdk/classes"), { recursive: true });
  await mkdir(join(output, "reference/node-sdk/classes/client"), {
    recursive: true,
  });
  await mkdir(join(content, "guide"), { recursive: true });
  await mkdir(join(output, "guide/case"), { recursive: true });
  await mkdir(join(output, "rust"), { recursive: true });
  await mkdir(join(output, "assets"), { recursive: true });
  await writeFile(join(root, "redirects.json"), "{}");
  await writeFile(
    source,
    "[method](/reference/node-sdk/classes/client/#method)",
  );
  await writeFile(page, '<h2 id="method">Method</h2>');
  const check = () =>
    checkBuilt({
      contentRoot: content,
      outputRoot: output,
      redirectsPath: join(root, "redirects.json"),
    });
  assert.deepEqual(
    (await check()).filter((failure) =>
      /built page is missing|built link target is missing|built anchor is missing|referenced asset is missing/u.test(
        failure,
      ),
    ),
    [],
  );
  await rm(page);
  assert.ok(
    (await check()).includes(
      "built page is missing: /reference/node-sdk/classes/client/",
    ),
  );
  await writeFile(page, '<h2 id="method">Method</h2>');
  await writeFile(
    source,
    "[wrong page](/reference/node-sdk/classes/Client/)\n[wrong anchor](/reference/node-sdk/classes/client/#Method)",
  );
  await writeFile(join(content, "guide/Case.md"), "# Case");
  await writeFile(join(output, "guide/case/index.html"), "Wrong route case");
  await writeFile(
    join(output, "rust/index.html"),
    '<link href="/assets/Styles.css">',
  );
  await writeFile(join(output, "assets/styles.css"), "body {}");
  const failures = await check();
  assert.ok(
    failures.includes(
      "reference/node-sdk/classes/Client.md: built link target is missing: /reference/node-sdk/classes/Client/",
    ),
  );
  assert.ok(
    failures.includes(
      "reference/node-sdk/classes/Client.md: built anchor is missing: /reference/node-sdk/classes/client/#Method",
    ),
  );
  assert.ok(failures.includes("built page is missing: /guide/Case/"));
  assert.ok(
    failures.includes(
      "rust/index.html: referenced asset is missing: /assets/Styles.css",
    ),
  );
});

test("TypeDoc explicit slugs preserve supported empty-slug symbols", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  await writeFile(join(root, "redirects.json"), "{}");
  for (const sdk of ["node", "browser", "agent", "react-native"]) {
    const directory = `reference/${sdk}-sdk/variables`;
    await mkdir(join(content, directory), { recursive: true });
    await mkdir(join(output, directory, "$"), { recursive: true });
    await writeFile(
      join(content, directory, "$.md"),
      `---\ntitle: "$"\nslug: ${directory}/$\n---\n`,
    );
    await writeFile(join(output, directory, "$/index.html"), "Dollar");
  }
  const check = async () =>
    (
      await checkBuilt({
        contentRoot: content,
        outputRoot: output,
        redirectsPath: join(root, "redirects.json"),
      })
    ).filter((failure) => failure.startsWith("built page is missing:"));
  assert.deepEqual(await check(), []);
  await rm(join(output, "reference/node-sdk/variables/$/index.html"));
  await writeFile(
    join(output, "reference/node-sdk/variables/index.html"),
    "Wrong page",
  );
  assert.deepEqual(await check(), [
    "built page is missing: /reference/node-sdk/variables/$/",
  ]);
});

test("source validation finds old code groups and broken relative links", async () => {
  const root = await fixture();
  const content = join(root, "content");
  await mkdir(content);
  await writeFile(
    join(content, "page.mdx"),
    ":::code-group\n[missing](./nope)\n",
  );
  const failures = await checkSource({ contentRoot: content });
  assert.equal(failures.length, 2);
  assert.match(failures.join("\n"), /literal :::code-group/);
  assert.match(failures.join("\n"), /broken local link/);
});

test("source validation requires the synchronized five-SDK tab set", async () => {
  const root = await fixture();
  const content = join(root, "content");
  await mkdir(content);
  await writeFile(
    join(content, "page.mdx"),
    '<Tabs syncKey="sdk"><TabItem label="Browser">b</TabItem><TabItem label="Node">n</TabItem><TabItem label="Kotlin">k</TabItem><TabItem label="Swift">s</TabItem></Tabs>',
  );
  assert.deepEqual(await checkSource({ contentRoot: content }), [
    "page.mdx: SDK tabs must be Browser, Node, React Native, Kotlin, Swift in that order, with an optional Agent tab last",
  ]);
});

test("source validation accepts an Agent tab after the platform SDKs", async () => {
  const root = await fixture();
  const content = join(root, "content");
  await mkdir(content);
  await writeFile(
    join(content, "page.mdx"),
    '<Tabs syncKey="sdk"><TabItem label="Browser">b</TabItem><TabItem label="Node">n</TabItem><TabItem label="React Native">rn</TabItem><TabItem label="Kotlin">k</TabItem><TabItem label="Swift">s</TabItem><TabItem label="Agent">a</TabItem></Tabs>',
  );
  assert.deepEqual(await checkSource({ contentRoot: content }), []);
});

test("built validation rejects incomplete llms exports and allows growth", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  await mkdir(content);
  await mkdir(join(output, "page"), { recursive: true });
  await writeFile(join(content, "page.md"), "# Page");
  await writeFile(join(output, "page/index.html"), "ok");
  await writeFile(
    join(root, "redirects.json"),
    JSON.stringify({ "/old/": "/missing/" }),
  );
  await writeFile(join(output, "llms-full.txt"), "# one\n");
  await writeFile(join(output, "llms.txt"), "ok");
  const failures = await checkBuilt({
    contentRoot: content,
    outputRoot: output,
    redirectsPath: join(root, "redirects.json"),
  });
  assert.ok(failures.some((failure) => failure.includes("redirect target")));
  assert.ok(
    failures.some((failure) => failure.includes("expected at least 120001")),
  );
  assert.ok(
    failures.some((failure) => failure.includes("expected at least 35")),
  );

  await writeFile(
    join(output, "llms-full.txt"),
    Array.from(
      { length: 35 },
      (_, index) => `# Page ${index}\n${"x".repeat(45_000)}`,
    ).join("\n"),
  );
  const expandedFailures = await checkBuilt({
    contentRoot: content,
    outputRoot: output,
    redirectsPath: join(root, "redirects.json"),
  });
  assert.ok(
    !expandedFailures.some((failure) => failure.startsWith("llms-full.txt")),
    expandedFailures.join("\n"),
  );
});

test("built validation checks focused exports and their index links", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  await mkdir(content);
  await mkdir(output);
  await writeFile(join(root, "redirects.json"), "{}");
  const guide = Array.from(
    { length: 35 },
    (_, i) => `# Page ${i}\n${"x".repeat(3500)}`,
  ).join("\n");
  const specs =
    "# Specification format\n```mermaid\nflowchart LR\nA --> B\n```\n" +
    Array.from({ length: 20 }, (_, i) => `# Spec ${i}\n`).join("");
  await writeFile(join(output, "llms-full.txt"), guide);
  await writeFile(join(output, "llms-small.txt"), "# Guide");
  await writeFile(join(output, "llms-specs.txt"), specs);
  await writeFile(
    join(output, "llms.txt"),
    "[Guide](/llms-full.txt)\n[Specs](/llms-specs.txt)",
  );
  const check = () =>
    checkBuilt({
      contentRoot: content,
      outputRoot: output,
      redirectsPath: join(root, "redirects.json"),
    });
  assert.deepEqual(
    (await check()).filter((failure) => failure.startsWith("llms")),
    [],
  );
  await writeFile(
    join(output, "llms-full.txt"),
    `${guide}\n# Specification format\n![](<data:image/svg+xml,bloat>)\n[Section titled Test](#test)`,
  );
  await writeFile(join(output, "llms-specs.txt"), "# Incomplete");
  await writeFile(join(output, "llms.txt"), "[Guide](/llms-full.txt)");
  const failures = await check();
  for (const expected of [
    "llms-full.txt contains embedded SVG data",
    "llms-full.txt contains heading navigation links",
    "llms-full.txt includes specs",
    "llms-specs.txt is missing Mermaid source",
    "llms-specs.txt is missing the specification format",
    "llms-specs.txt has fewer than 20 pages",
    "llms.txt does not link to llms-specs.txt",
  ])
    assert.ok(failures.includes(expected), expected);
});

test("built validation rejects review records in composed specs", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  await mkdir(content);
  await mkdir(join(output, "page"), { recursive: true });
  await mkdir(join(output, "specs/001"), { recursive: true });
  await writeFile(join(content, "page.md"), "# Page");
  await writeFile(join(output, "page/index.html"), "ok");
  await writeFile(
    join(output, "specs/001/index.html"),
    '<h2 id="review-record">Review record</h2>',
  );
  await writeFile(join(root, "redirects.json"), "{}");
  await writeFile(
    join(output, "llms-full.txt"),
    Array.from(
      { length: 35 },
      (_, index) => `# Page ${index}\n${"x".repeat(3500)}`,
    ).join("\n"),
  );
  await writeFile(join(output, "llms.txt"), "ok");
  const failures = await checkBuilt({
    contentRoot: content,
    outputRoot: output,
    redirectsPath: join(root, "redirects.json"),
  });
  assert.ok(
    failures.includes(
      "specs/001/index.html: built spec exposes a review record or log",
    ),
  );
});

test("built validation rejects an llms file without resolved examples", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  const examples = join(root, "examples");
  await mkdir(content);
  await mkdir(join(output, "page"), { recursive: true });
  await mkdir(examples);
  await writeFile(
    join(content, "page.md"),
    '# Page\n\n\x60\x60\x60ts source="example.ts" region="kept"\n\x60\x60\x60',
  );
  await writeFile(
    join(examples, "example.ts"),
    "// #region kept\nconst retained = true;\n// #endregion kept",
  );
  await writeFile(join(output, "page/index.html"), "ok");
  await writeFile(join(root, "redirects.json"), "{}");
  await writeFile(
    join(output, "llms-full.txt"),
    Array.from(
      { length: 35 },
      (_, index) => "# Page " + index + "\n" + "x".repeat(4000),
    ).join("\n"),
  );
  await writeFile(join(output, "llms.txt"), "ok");
  const failures = await checkBuilt({
    contentRoot: content,
    outputRoot: output,
    redirectsPath: join(root, "redirects.json"),
    examplesRoot: examples,
  });
  assert.ok(
    failures.includes(
      "llms-full.txt is missing resolved snippet example.ts#kept",
    ),
  );
});

test("built validation checks native entrypoint assets", async () => {
  const root = await fixture();
  const content = join(root, "content");
  const output = join(root, "site");
  await mkdir(content);
  await mkdir(join(output, "page"), { recursive: true });
  await mkdir(join(output, "rust/xmtp_mls"), { recursive: true });
  await mkdir(join(output, "reference/kotlin"), { recursive: true });
  await mkdir(join(output, "reference/swift"), { recursive: true });
  await writeFile(join(content, "page.md"), "# Page");
  await writeFile(join(output, "page/index.html"), "ok");
  await writeFile(
    join(output, "rust/xmtp_mls/index.html"),
    '<link rel="stylesheet" href="missing.css">',
  );
  await writeFile(join(output, "rust/index.html"), "Redirect");
  await writeFile(join(output, "reference/kotlin/index.html"), "Kotlin");
  await writeFile(join(output, "reference/swift/index.html"), "Swift");
  await writeFile(join(root, "redirects.json"), "{}");
  await writeFile(
    join(output, "llms-full.txt"),
    Array.from(
      { length: 35 },
      (_, index) => `# Page ${index}\n${"x".repeat(4000)}`,
    ).join("\n"),
  );
  await writeFile(join(output, "llms.txt"), "ok");
  const failures = await checkBuilt({
    contentRoot: content,
    outputRoot: output,
    redirectsPath: join(root, "redirects.json"),
  });
  assert.ok(
    failures.includes(
      "rust/xmtp_mls/index.html: referenced asset is missing: missing.css",
    ),
  );
});
