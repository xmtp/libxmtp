import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { serveStatic } from "../scripts/check-serve.mjs";

test("static server serves directory indexes and returns 404", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-serve-test-"));
  await mkdir(join(root, "guide"));
  await writeFile(join(root, "guide/index.html"), "Guide");
  const server = await serveStatic({ root, port: 0 });
  try {
    const address = server.address();
    const base = `http://127.0.0.1:${address.port}`;
    const page = await fetch(`${base}/guide/`);
    assert.equal(page.status, 200);
    assert.equal(await page.text(), "Guide");
    assert.equal((await fetch(`${base}/missing/`)).status, 404);
    assert.equal((await fetch(`${base}/%XX`)).status, 400);
    assert.equal(
      (await fetch(`${base}/..%2f${root.split("/").at(-1)}-other/file`)).status,
      403,
    );
  } finally {
    server.close();
  }
});
