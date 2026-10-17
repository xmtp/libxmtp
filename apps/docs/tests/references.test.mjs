import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { installReferences } from "../scripts/references.mjs";

async function referenceFixture(
  t,
  { module = "xmtpsdk", skipNative = false } = {},
) {
  const root = await mkdtemp(join(tmpdir(), "xmtp-references-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const previous = process.env.DOCS_SKIP_NATIVE_REFERENCES;
  process.env.DOCS_SKIP_NATIVE_REFERENCES = skipNative ? "1" : "";
  t.after(() => {
    if (previous === undefined) delete process.env.DOCS_SKIP_NATIVE_REFERENCES;
    else process.env.DOCS_SKIP_NATIVE_REFERENCES = previous;
  });
  const generatedRoot = join(root, "generated");
  const siteRoot = join(root, "site");
  await mkdir(join(generatedRoot, "rust/xmtp_mls"), { recursive: true });
  await writeFile(join(generatedRoot, "rust/xmtp_mls/index.html"), "Rust");
  if (!skipNative) {
    await mkdir(join(generatedRoot, "kotlin"), { recursive: true });
    await writeFile(join(generatedRoot, "kotlin/index.html"), "Kotlin");
    await mkdir(join(generatedRoot, "swift"), { recursive: true });
    await writeFile(join(generatedRoot, "swift/index.html"), "DocC");
    if (module) {
      await mkdir(join(generatedRoot, "swift/documentation", module), {
        recursive: true,
      });
      await writeFile(
        join(generatedRoot, "swift/documentation", module, "index.html"),
        "Swift module",
      );
    }
  }
  return { generatedRoot, siteRoot };
}

test("reference installation accepts the generated XmtpSdk module", async (t) => {
  const options = await referenceFixture(t);
  await installReferences(options);
  assert.equal(
    await readFile(
      join(
        options.siteRoot,
        "reference/swift/documentation/xmtpsdk/index.html",
      ),
      "utf8",
    ),
    "Swift module",
  );
});

for (const module of [null, "xmtpios"]) {
  test(`reference installation rejects ${module ?? "missing"} Swift module output`, async (t) => {
    const options = await referenceFixture(t, { module });
    await assert.rejects(
      installReferences(options),
      /Swift module index is missing: .*\/documentation\/xmtpsdk\/index\.html/,
    );
  });
}

test("pull-request reference installation needs only Rust", async (t) => {
  const options = await referenceFixture(t, { skipNative: true });
  await installReferences(options);
  assert.match(
    await readFile(join(options.siteRoot, "rust/index.html"), "utf8"),
    /url=\.\/xmtp_mls\//,
  );
  await assert.rejects(
    readFile(join(options.siteRoot, "reference/swift/index.html")),
    /ENOENT/,
  );
});
