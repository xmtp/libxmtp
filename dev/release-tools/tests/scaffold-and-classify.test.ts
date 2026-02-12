import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { scaffoldNotes } from "../src/commands/scaffold-notes";
import { classifyNoteFiles } from "../src/lib/classify-notes";

describe("scaffold → classify end-to-end", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-e2e-"),
    );
    // Create iOS SDK structure
    fs.mkdirSync(path.join(tmpDir, "sdks/ios"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      `Pod::Spec.new do |spec|\n  spec.version      = "2.0.0"\nend\n`,
    );
    // Create Android SDK structure
    fs.mkdirSync(path.join(tmpDir, "sdks/android"), { recursive: true });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/android/gradle.properties"),
      `version=3.1.0\n`,
    );
    fs.mkdirSync(path.join(tmpDir, "docs/release-notes"), { recursive: true });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  function readNote(filePath: string) {
    const repoRelative = path.relative(tmpDir, filePath);
    const content = fs.readFileSync(filePath, "utf-8");
    return { path: repoRelative, content };
  }

  it("classifies an unmodified scaffold with tag as empty", () => {
    const filePath = scaffoldNotes("ios", tmpDir, "1.9.0", "ios-1.9.0");
    const result = classifyNoteFiles([readNote(filePath)]);

    expect(result.empty).toHaveLength(1);
    expect(result.content).toHaveLength(0);
    expect(result.empty[0]).toEqual({
      sdk: "ios",
      filePath: "docs/release-notes/ios/2.0.0.md",
      previousReleaseVersion: "1.9.0",
      previousReleaseTag: "ios-1.9.0",
    });
  });

  it("classifies an unmodified scaffold without tag as empty", () => {
    const filePath = scaffoldNotes("ios", tmpDir, "1.9.0", null);
    const result = classifyNoteFiles([readNote(filePath)]);

    expect(result.empty).toHaveLength(1);
    expect(result.content).toHaveLength(0);
    expect(result.empty[0]).toEqual({
      sdk: "ios",
      filePath: "docs/release-notes/ios/2.0.0.md",
      previousReleaseVersion: "1.9.0",
      previousReleaseTag: null,
    });
  });

  it("classifies a modified scaffold with tag as content", () => {
    const filePath = scaffoldNotes("android", tmpDir, "3.0.0", "android-3.0.0");
    const original = fs.readFileSync(filePath, "utf-8");
    const modified = original.replace(
      "<!-- AI-generated draft - please review and edit -->",
      "This release improves group messaging performance.",
    );
    fs.writeFileSync(filePath, modified);

    const result = classifyNoteFiles([readNote(filePath)]);

    expect(result.empty).toHaveLength(0);
    expect(result.content).toHaveLength(1);
    expect(result.content[0]).toEqual({
      sdk: "android",
      filePath: "docs/release-notes/android/3.1.0.md",
      previousReleaseVersion: "3.0.0",
      previousReleaseTag: "android-3.0.0",
    });
  });

  it("classifies a modified scaffold without tag as content", () => {
    const filePath = scaffoldNotes("android", tmpDir, "3.0.0", null);
    const original = fs.readFileSync(filePath, "utf-8");
    const modified = original.replace(
      "<!-- AI-generated draft - please review and edit -->",
      "Added encryption support.",
    );
    fs.writeFileSync(filePath, modified);

    const result = classifyNoteFiles([readNote(filePath)]);

    expect(result.empty).toHaveLength(0);
    expect(result.content).toHaveLength(1);
    expect(result.content[0]).toEqual({
      sdk: "android",
      filePath: "docs/release-notes/android/3.1.0.md",
      previousReleaseVersion: "3.0.0",
      previousReleaseTag: null,
    });
  });

  it("classifies a mix of empty and modified scaffolds across SDKs", () => {
    const iosPath = scaffoldNotes("ios", tmpDir, "1.9.0", "ios-1.9.0");
    const androidPath = scaffoldNotes("android", tmpDir, "3.0.0", "android-3.0.0");

    // Modify only the Android notes
    const androidContent = fs.readFileSync(androidPath, "utf-8");
    fs.writeFileSync(
      androidPath,
      androidContent.replace(
        "<!-- Describe what changed in this release -->",
        "- Improved sync reliability\n- Fixed crash on startup",
      ),
    );

    const result = classifyNoteFiles([
      readNote(iosPath),
      readNote(androidPath),
    ]);

    expect(result.empty).toHaveLength(1);
    expect(result.empty[0].sdk).toBe("ios");
    expect(result.empty[0].previousReleaseTag).toBe("ios-1.9.0");

    expect(result.content).toHaveLength(1);
    expect(result.content[0].sdk).toBe("android");
    expect(result.content[0].previousReleaseTag).toBe("android-3.0.0");
  });

  it("classifies a mix with and without previous tags", () => {
    const iosPath = scaffoldNotes("ios", tmpDir, "1.9.0", "ios-1.9.0");
    const androidPath = scaffoldNotes("android", tmpDir, "3.0.0", null);

    // Both are unmodified scaffolds
    const result = classifyNoteFiles([
      readNote(iosPath),
      readNote(androidPath),
    ]);

    expect(result.empty).toHaveLength(2);
    expect(result.content).toHaveLength(0);

    const ios = result.empty.find((n) => n.sdk === "ios")!;
    expect(ios.previousReleaseVersion).toBe("1.9.0");
    expect(ios.previousReleaseTag).toBe("ios-1.9.0");

    const android = result.empty.find((n) => n.sdk === "android")!;
    expect(android.previousReleaseVersion).toBe("3.0.0");
    expect(android.previousReleaseTag).toBeNull();
  });
});
