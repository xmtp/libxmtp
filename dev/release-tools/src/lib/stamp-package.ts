import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

const sha256 = (bytes: string | Buffer) =>
  createHash("sha256").update(bytes).digest("hex");

/**
 * Set the version of a staged SDK product before `npm publish`. A product with
 * an `sdk-contract.json` receipt must match every recorded checksum first; the
 * receipt then records the new `package.json` checksum.
 */
export function stampPackage(directory: string, version: string): void {
  if (!VERSION.test(version)) throw new Error("Invalid npm package version");
  const manifestFile = path.join(directory, "package.json");
  const manifest = JSON.parse(fs.readFileSync(manifestFile, "utf-8")) as {
    version?: string;
  };
  const receiptFile = path.join(directory, "sdk-contract.json");
  const receipt = fs.existsSync(receiptFile)
    ? (JSON.parse(fs.readFileSync(receiptFile, "utf-8")) as {
        assets?: Record<string, string>;
      })
    : undefined;
  if (receipt) {
    if (!receipt.assets?.["package.json"])
      throw new Error("SDK product has no package manifest checksum");
    for (const [file, expected] of Object.entries(receipt.assets)) {
      if (sha256(fs.readFileSync(path.join(directory, file))) !== expected)
        throw new Error(`SDK product asset mismatch: ${file}`);
    }
  }
  manifest.version = version;
  const bytes = JSON.stringify(manifest, null, 2);
  fs.writeFileSync(manifestFile, bytes);
  if (receipt?.assets) {
    receipt.assets["package.json"] = sha256(bytes);
    fs.writeFileSync(receiptFile, JSON.stringify(receipt, null, 2) + "\n");
  }
}
