import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { checkGeneratedAssets } from "./check-generated-assets.mjs";

const generated = mkdtempSync(join(tmpdir(), "sdk-generated-assets-"));
const tree = "typescript-pure";
const root = join(generated, tree);
const wasm = Buffer.from([0, 97, 115, 109]);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
try {
  mkdirSync(join(root, "snippets"), { recursive: true });
  writeFileSync(join(root, "xmtp_sdk_bg.wasm"), wasm);
  writeFileSync(join(root, "snippets/binding.js"), "export const bind = 1;\n");
  const record = {
    files: {
      "xmtp_sdk_bg.wasm": digest(wasm),
      "snippets/binding.js": digest("export const bind = 1;\n"),
    },
  };
  writeFileSync(join(root, "sdk-contract.json"), JSON.stringify(record));
  checkGeneratedAssets(generated, tree);
  writeFileSync(join(root, "unlisted.js"), "export const stale = true;\n");
  assert.throws(
    () => checkGeneratedAssets(generated, tree),
    /asset set mismatch/,
  );
  rmSync(join(root, "unlisted.js"));
  writeFileSync(join(root, "snippets/unlisted.js"), "extra");
  assert.throws(
    () => checkGeneratedAssets(generated, tree),
    /asset set mismatch/,
  );
  rmSync(join(root, "snippets/unlisted.js"));
  writeFileSync(join(root, "xmtp_sdk_bg.wasm"), "changed");
  assert.throws(() => checkGeneratedAssets(generated, tree), /asset mismatch/);
  rmSync(join(root, "xmtp_sdk_bg.wasm"));
  assert.throws(
    () => checkGeneratedAssets(generated, tree),
    /asset set mismatch/,
  );
  writeFileSync(join(root, "xmtp_sdk_bg.wasm"), wasm);
  checkGeneratedAssets(generated, tree);
  console.log(
    "Generated assets: valid, extra, nested extra, changed, missing and restored checks pass",
  );
} finally {
  rmSync(generated, { recursive: true, force: true });
}
