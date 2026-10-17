import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { stampPackage } from "../src/lib/stamp-package";

const hash = (bytes: string | Buffer) =>
  createHash("sha256").update(bytes).digest("hex");

describe("stampPackage", () => {
  let directory: string;

  beforeEach(() => {
    directory = fs.mkdtempSync(path.join(os.tmpdir(), "sdk-stamp-"));
  });

  afterEach(() => {
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("changes the packed manifest and keeps product checksums valid", () => {
    const manifest = '{"name":"@xmtp/node-sdk","version":"8.0.0"}\n';
    const payload = "export const marker = 'native product';\n";
    const metadata = {
      contract: "fixed",
      platforms: { target: "fixed" },
      assets: { "package.json": hash(manifest), "entry.js": hash(payload) },
    };
    fs.writeFileSync(path.join(directory, "package.json"), manifest);
    fs.writeFileSync(path.join(directory, "entry.js"), payload);
    fs.writeFileSync(
      path.join(directory, "sdk-contract.json"),
      JSON.stringify(metadata),
    );

    stampPackage(directory, "8.0.1-rc.1");
    const bytes = fs.readFileSync(path.join(directory, "package.json"));
    expect(JSON.parse(bytes.toString()).version).toBe("8.0.1-rc.1");
    const receipt = JSON.parse(
      fs.readFileSync(path.join(directory, "sdk-contract.json"), "utf-8"),
    );
    expect(receipt.assets["package.json"]).toBe(hash(bytes));
    expect(receipt.assets["entry.js"]).toBe(hash(payload));
    expect(receipt.contract).toBe(metadata.contract);
    expect(receipt.platforms).toEqual(metadata.platforms);

    fs.writeFileSync(path.join(directory, "entry.js"), "tampered");
    expect(() => stampPackage(directory, "8.0.2")).toThrow(
      "SDK product asset mismatch: entry.js",
    );
    expect(fs.readFileSync(path.join(directory, "package.json"))).toEqual(
      bytes,
    );
  });

  it("refuses a version that npm would not accept", () => {
    fs.writeFileSync(path.join(directory, "package.json"), "{}");
    expect(() => stampPackage(directory, "8.0")).toThrow(
      "Invalid npm package version",
    );
  });
});
