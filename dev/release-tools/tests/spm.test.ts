import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { updateSpmChecksum } from "../src/lib/spm";

describe("the shared Apple archive receipt", () => {
  let root: string;
  let packagePath: string;
  const url = "https://example.com/releases/ios-8.0.0/XmtpSdkFFI.zip";
  const checksum = createHash("sha256")
    .update("test archive bytes")
    .digest("hex");

  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-spm-"));
    fs.mkdirSync(path.join(root, "sdks/ios"), { recursive: true });
    packagePath = path.join(root, "Package.swift");
    fs.writeFileSync(packagePath, "// consumer manifest remains unchanged\n");
  });

  afterEach(() => fs.rmSync(root, { recursive: true }));

  it("writes the supplied archive identity for both package managers", () => {
    updateSpmChecksum(packagePath, url, checksum.toUpperCase());
    const receipt = JSON.parse(
      fs.readFileSync(
        path.join(root, "sdks/ios/ReleaseArtifacts.json"),
        "utf-8",
      ),
    );
    expect(receipt).toEqual({ url, sha256: checksum });
    expect(fs.readFileSync(packagePath, "utf-8")).toBe(
      "// consumer manifest remains unchanged\n",
    );
  });

  it("rejects the old split archive and an invalid checksum before writing", () => {
    expect(() =>
      updateSpmChecksum(
        packagePath,
        "https://example.com/LibXMTPSwiftFFI.zip",
        checksum,
      ),
    ).toThrow();
    expect(() => updateSpmChecksum(packagePath, url, "missing")).toThrow();
    expect(
      fs.existsSync(path.join(root, "sdks/ios/ReleaseArtifacts.json")),
    ).toBe(false);
  });
});
