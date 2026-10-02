#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const directory = resolve(process.argv[2]);
const version = process.argv[3];
if (
  !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(
    version ?? "",
  )
)
  throw new Error("Invalid npm package version");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const manifestFile = join(directory, "package.json");
const manifest = JSON.parse(readFileSync(manifestFile));
const receiptFile = join(directory, "sdk-contract.json");
const receipt = existsSync(receiptFile)
  ? JSON.parse(readFileSync(receiptFile))
  : undefined;
if (receipt) {
  if (!receipt.assets?.["package.json"])
    throw new Error("SDK product has no package manifest checksum");
  for (const [path, expected] of Object.entries(receipt.assets)) {
    if (hash(readFileSync(join(directory, path))) !== expected)
      throw new Error(`SDK product asset mismatch: ${path}`);
  }
}
manifest.version = version;
const bytes = JSON.stringify(manifest, null, 2);
writeFileSync(manifestFile, bytes);
if (receipt) {
  receipt.assets["package.json"] = hash(bytes);
  writeFileSync(receiptFile, JSON.stringify(receipt, null, 2) + "\n");
}
