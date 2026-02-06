import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { updateSpmChecksum } from "../src/lib/spm";

const SAMPLE_PACKAGE_SWIFT = `// swift-tools-version: 5.6
import Foundation
import PackageDescription

let thisPackagePath = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let useLocalBinary = FileManager.default.fileExists(
    atPath: "\\(thisPackagePath)/.build/LibXMTPSwiftFFI.xcframework"
)

let package = Package(
    name: "XMTPiOS",
    platforms: [.iOS(.v14), .macOS(.v11)],
    targets: [
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFI",
                path: ".build/LibXMTPSwiftFFI.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFI",
                url: "https://github.com/xmtp/libxmtp/releases/download/ios-4.9.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",
                checksum: "oldchecksum123"
            ),
    ]
)
`;

describe("updateSpmChecksum", () => {
  let tmpDir: string;
  let packagePath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-spm-"));
    packagePath = path.join(tmpDir, "Package.swift");
    fs.writeFileSync(packagePath, SAMPLE_PACKAGE_SWIFT);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("updates the url and checksum", () => {
    updateSpmChecksum(
      packagePath,
      "https://github.com/xmtp/libxmtp/releases/download/ios-4.10.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",
      "newchecksum456",
    );
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain("ios-4.10.0-libxmtp");
    expect(content).toContain('checksum: "newchecksum456"');
    expect(content).not.toContain("oldchecksum123");
    expect(content).not.toContain("ios-4.9.0-libxmtp");
  });

  it("preserves the local binary target path", () => {
    updateSpmChecksum(packagePath, "https://example.com/new.zip", "abc");
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain('path: ".build/LibXMTPSwiftFFI.xcframework"');
  });

  it("preserves the conditional logic", () => {
    updateSpmChecksum(packagePath, "https://example.com/new.zip", "abc");
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain("useLocalBinary");
    expect(content).toContain("FileManager.default.fileExists");
  });

  it("handles widely spaced multiline formatting", () => {
    const widelySpaced = `// swift-tools-version: 5.6
import PackageDescription

let package = Package(
    targets: [
        .binaryTarget(
            name: "LibXMTPSwiftFFI",

            url:
                "https://github.com/xmtp/libxmtp/releases/download/ios-4.9.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",

            checksum:
                "oldchecksum123"
        ),
    ]
)
`;
    fs.writeFileSync(packagePath, widelySpaced);
    updateSpmChecksum(
      packagePath,
      "https://example.com/new.zip",
      "newchecksum",
    );
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain('"https://example.com/new.zip"');
    expect(content).toContain('"newchecksum"');
    expect(content).not.toContain("oldchecksum123");
    expect(content).not.toContain("ios-4.9.0-libxmtp");
  });

  it("throws if url pattern is not found", () => {
    fs.writeFileSync(packagePath, "no url here\n");
    expect(() =>
      updateSpmChecksum(packagePath, "https://example.com/new.zip", "abc"),
    ).toThrow();
  });
});
