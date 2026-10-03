import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function checkGeneratedAssets(generated, tree, record) {
  const root = join(generated, tree);
  record ??= JSON.parse(readFileSync(join(root, "sdk-contract.json")));
  const collect = (folder) =>
    readdirSync(folder, { withFileTypes: true }).flatMap((item) => {
      if (item.name === "node_modules" && item.isDirectory()) return [];
      const path = join(folder, item.name);
      return item.isDirectory() ? collect(path) : [relative(root, path)];
    });
  const actual = collect(root).filter((name) => name !== "sdk-contract.json");
  assert.deepEqual(
    actual.sort(),
    Object.keys(record.files).sort(),
    `SDK generated asset set mismatch: ${tree}`,
  );
  for (const [path, expected] of Object.entries(record.files)) {
    const digest = createHash("sha256")
      .update(readFileSync(join(root, path)))
      .digest("hex");
    assert.equal(digest, expected, `SDK generated asset mismatch: ${path}`);
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const generated = resolve(process.argv[2] ?? "target/sdk-generated");
  const trees = process.argv.slice(3);
  for (const tree of trees.length
    ? trees
    : ["typescript-napi", "typescript-wasm", "typescript-pure"])
    checkGeneratedAssets(generated, tree);
  console.log("Generated asset sets and bytes match their contracts");
}
