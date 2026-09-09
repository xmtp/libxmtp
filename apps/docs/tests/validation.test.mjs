import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { checkParity } from "../scripts/check-parity.mjs";
import { checkBuilt, checkSource } from "../scripts/check-site.mjs";
import {
  assertTypeDocValidation,
  contentRoute,
  normalizeText,
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

test("normalization keeps prose and ignores port markup", () => {
  assert.equal(
    normalizeText(
      ':::note[Read]\nHello   world\n:::\n```js title="x"\na()\n```',
    ),
    "Hello world ``` a() ```",
  );
});

test("content routes support index pages", () => {
  assert.equal(contentRoute("/docs", "/docs/sdk/index.mdx"), "/sdk/");
  assert.equal(contentRoute("/docs", "/docs/quickstart.md"), "/quickstart/");
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

test("source validation requires the synchronized four-SDK tab set", async () => {
  const root = await fixture();
  const content = join(root, "content");
  await mkdir(content);
  await writeFile(
    join(content, "page.mdx"),
    '<Tabs syncKey="sdk"><TabItem label="Node">x</TabItem><TabItem label="Swift">x</TabItem></Tabs>',
  );
  assert.deepEqual(await checkSource({ contentRoot: content }), [
    "page.mdx: SDK tabs must be Browser, Node, Kotlin, Swift in that order",
  ]);
});

test("frozen parity blocks detect a rephrase without an old checkout", async () => {
  const root = await fixture();
  const parity = join(root, "parity");
  const content = join(root, "content");
  await mkdir(parity);
  await mkdir(content);
  for (const name of ["start", "sdk", "other"])
    await writeFile(
      join(parity, `${name}.json`),
      name === "sdk"
        ? JSON.stringify([
            {
              source: "old.mdx",
              target: "new.mdx",
              blocks: ["Keep this exact sentence."],
            },
          ])
        : "[]",
    );
  await writeFile(join(content, "new.mdx"), "Keep this changed sentence.");
  assert.deepEqual(
    await checkParity({ parityRoot: parity, contentRoot: content }),
    ["sdk.json[0]: retained block 1 changed in new.mdx"],
  );
});

test("frozen parity expands compiled example regions before comparison", async () => {
  const root = await fixture();
  const parity = join(root, "parity");
  const content = join(root, "content");
  const examples = join(root, "examples");
  await mkdir(parity);
  await mkdir(content);
  await mkdir(examples);
  for (const name of ["start", "sdk", "other"])
    await writeFile(
      join(parity, name + ".json"),
      name === "sdk"
        ? JSON.stringify([
            {
              source: "old.mdx",
              target: "new.mdx",
              blocks: ["const retained = true;"],
            },
          ])
        : "[]",
    );
  await writeFile(
    join(content, "new.mdx"),
    '\x60\x60\x60ts source="example.ts" region="kept"\n\x60\x60\x60',
  );
  await writeFile(
    join(examples, "example.ts"),
    "// #region kept\nconst retained = true;\n// #endregion kept",
  );
  assert.deepEqual(
    await checkParity({
      parityRoot: parity,
      contentRoot: content,
      examplesRoot: examples,
    }),
    [],
  );
});

test("built validation checks redirect targets and llms bounds", async () => {
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
    failures.some((failure) => failure.includes("expected 120001-899999")),
  );
  assert.ok(
    failures.some((failure) => failure.includes("expected at least 35")),
  );
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
