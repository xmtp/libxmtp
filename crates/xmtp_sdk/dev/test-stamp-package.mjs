import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

test("release stamping changes the packed manifest and keeps product checksums valid", () => {
  const directory = mkdtempSync(join(tmpdir(), "sdk-stamp-"));
  const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
  const manifest = '{"name":"@xmtp/node-sdk","version":"8.0.0"}\n';
  const payload = "export const marker = 'native product';\n";
  const metadata = {
    contract: "fixed",
    platforms: { target: "fixed" },
    assets: { "package.json": hash(manifest), "entry.js": hash(payload) },
  };
  const command = (version) =>
    spawnSync(
      process.execPath,
      [
        new URL("./stamp-package.mjs", import.meta.url).pathname,
        directory,
        version,
      ],
      { encoding: "utf8" },
    );
  try {
    writeFileSync(join(directory, "package.json"), manifest);
    writeFileSync(join(directory, "entry.js"), payload);
    writeFileSync(
      join(directory, "sdk-contract.json"),
      JSON.stringify(metadata),
    );
    const stamped = command("8.0.1-rc.1");
    assert.equal(stamped.status, 0, stamped.stderr);
    const bytes = readFileSync(join(directory, "package.json"));
    assert.equal(JSON.parse(bytes).version, "8.0.1-rc.1");
    const receipt = JSON.parse(
      readFileSync(join(directory, "sdk-contract.json")),
    );
    assert.equal(receipt.assets["package.json"], hash(bytes));
    assert.equal(receipt.assets["entry.js"], hash(payload));
    assert.equal(receipt.contract, metadata.contract);
    assert.deepEqual(receipt.platforms, metadata.platforms);
    writeFileSync(join(directory, "entry.js"), "tampered");
    const failed = command("8.0.2");
    assert.notEqual(failed.status, 0);
    assert.match(failed.stderr, /asset mismatch: entry.js/);
    assert.deepEqual(readFileSync(join(directory, "package.json")), bytes);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
