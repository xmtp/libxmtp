import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildSite, checkFreshness } from "../scripts/site-freshness.mjs";

async function fixture(t) {
  const repositoryRoot = await mkdtemp(join(tmpdir(), "docs-freshness-"));
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  execFileSync("git", ["init", "--quiet"], { cwd: repositoryRoot });
  const distRoot = join(repositoryRoot, "dist");
  await writeFile(join(repositoryRoot, ".gitignore"), "dist/\n");
  await writeFile(join(repositoryRoot, "guide.md"), "https://example.com/ok");
  execFileSync("git", ["add", ".gitignore", "guide.md"], {
    cwd: repositoryRoot,
  });
  const options = { repositoryRoot, distRoot };
  const build = async () => {
    await mkdir(distRoot, { recursive: true });
    await writeFile(
      join(distRoot, "index.html"),
      `<a href="${await readFile(join(repositoryRoot, "guide.md"), "utf8")}">link</a>`,
    );
  };
  await buildSite({ ...options, build });
  return { ...options, build };
}

test("fresh output passes; changed source fails until rebuilt; restored source passes", async (t) => {
  const options = await fixture(t);
  await checkFreshness(options);
  await writeFile(
    join(options.repositoryRoot, "guide.md"),
    "https://example.com/missing",
  );
  await assert.rejects(checkFreshness(options), /stale/);
  await buildSite(options);
  await checkFreshness(options);
  assert.match(
    await readFile(join(options.distRoot, "index.html"), "utf8"),
    /missing/,
  );
  await writeFile(
    join(options.repositoryRoot, "guide.md"),
    "https://example.com/ok",
  );
  await assert.rejects(checkFreshness(options), /stale/);
  await buildSite(options);
  await checkFreshness(options);
});

test("new, deleted, and changed output files invalidate the stamp", async (t) => {
  const options = await fixture(t);
  const added = join(options.repositoryRoot, "new.md");
  await writeFile(added, "https://example.com/new");
  await assert.rejects(checkFreshness(options), /stale/);
  await rm(added);
  await checkFreshness(options);
  const guide = join(options.repositoryRoot, "guide.md");
  const original = await readFile(guide);
  await rm(guide);
  await assert.rejects(checkFreshness(options), /stale/);
  await writeFile(guide, original);
  await checkFreshness(options);
  await writeFile(join(options.distRoot, "extra.html"), "unverified output");
  await assert.rejects(checkFreshness(options), /stale/);
});

test("missing stamps, empty output, and failed builds cannot pass", async (t) => {
  const options = await fixture(t);
  await assert.rejects(
    buildSite({
      ...options,
      build: () => {
        throw new Error("build failed");
      },
    }),
    /build failed/,
  );
  await assert.rejects(checkFreshness(options), /stamp is missing/);
  await assert.rejects(
    buildSite({
      ...options,
      build: () => writeFile(join(options.distRoot, "index.html"), ""),
    }),
    /entrypoint is empty/,
  );
  await assert.rejects(checkFreshness(options), /stamp is missing/);
});

test("source edits during a build do not receive a stamp", async (t) => {
  const options = await fixture(t);
  await assert.rejects(
    buildSite({
      ...options,
      build: async () => {
        await options.build();
        await writeFile(
          join(options.repositoryRoot, "guide.md"),
          "changed during build",
        );
      },
    }),
    /changed during the build/,
  );
  await assert.rejects(checkFreshness(options), /stamp is missing/);
});

test("directory symlinks use their stored link value", async (t) => {
  const options = await fixture(t);
  const link = join(options.repositoryRoot, "skills");
  await symlink(".", link);
  await buildSite(options);
  await checkFreshness(options);
  await rm(link);
  await symlink("dist", link);
  await assert.rejects(checkFreshness(options), /stale/);
});
