import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { scaffoldNotes } from "../../src/commands/scaffold-notes";

describe("scaffoldNotes", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-notes-"));
    // Create directory structure
    fs.mkdirSync(path.join(tmpDir, "sdks/ios"), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, "docs/release-notes"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      `Pod::Spec.new do |spec|\n  spec.version      = "4.10.0"\nend\n`,
    );
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("creates release notes from a since tag", () => {
    const outputPath = scaffoldNotes("ios", tmpDir, "4.9.0", "ios-4.9.0");
    expect(outputPath).toContain("docs/release-notes/ios/4.10.0.md");

    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain('previous_release_version = "4.9.0"');
    expect(content).toContain('previous_release_tag = "ios-4.9.0"');
    expect(content).toContain('sdk = "ios"');
  });

  it("handles no previous tag gracefully", () => {
    const outputPath = scaffoldNotes("ios", tmpDir, "4.9.0", null);
    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain('previous_release_version = "4.9.0"');
    expect(content).not.toContain("previous_release_tag");
    expect(content).toContain('sdk = "ios"');
  });
});
