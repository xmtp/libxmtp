import { describe, it, expect } from "vitest";
import {
  parseFrontmatter,
  isEmptyScaffold,
  classifyNoteFiles,
} from "../src/lib/classify-notes";

describe("parseFrontmatter", () => {
  it("parses valid frontmatter", () => {
    const text = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# Some content`;

    expect(parseFrontmatter(text)).toEqual({
      sdk: "ios",
      previousReleaseTag: "ios-0.9.0",
    });
  });

  it("returns nulls for missing fields", () => {
    const text = `---
sdk: android
---

# Content`;

    expect(parseFrontmatter(text)).toEqual({
      sdk: "android",
      previousReleaseTag: null,
    });
  });

  it("returns nulls when no frontmatter", () => {
    const text = `# Just a markdown file

Some content here.`;

    expect(parseFrontmatter(text)).toEqual({
      sdk: null,
      previousReleaseTag: null,
    });
  });

  it("treats null string values as null", () => {
    const text = `---
sdk: ios
previous_release_tag: null
---

# Content`;

    expect(parseFrontmatter(text)).toEqual({
      sdk: "ios",
      previousReleaseTag: null,
    });
  });
});

describe("isEmptyScaffold", () => {
  it("returns true for scaffold with only HTML comments and headers", () => {
    const text = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# iOS SDK 1.0.0

## Highlights

<!-- AI-generated draft - please review and edit -->

## What's Changed

<!-- Describe what changed in this release -->

## Breaking Changes

<!-- List any breaking changes here -->

## New Features

<!-- List new features here -->

## Bug Fixes

<!-- List bug fixes here -->
`;

    expect(isEmptyScaffold(text)).toBe(true);
  });

  it("returns false for file with real content", () => {
    const text = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# iOS SDK 1.0.0

## Highlights

This release adds group messaging support.

## What's Changed

- Added group messaging
- Fixed connection issues
`;

    expect(isEmptyScaffold(text)).toBe(false);
  });

  it("returns false for file with content below comments", () => {
    const text = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# iOS SDK 1.0.0

## Highlights

<!-- AI-generated draft -->

Some actual content here.
`;

    expect(isEmptyScaffold(text)).toBe(false);
  });

  it("returns true for empty file", () => {
    expect(isEmptyScaffold("")).toBe(true);
  });

  it("returns true for file with only whitespace after stripping", () => {
    const text = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# Header

`;

    expect(isEmptyScaffold(text)).toBe(true);
  });
});

describe("classifyNoteFiles", () => {
  const emptyScaffold = `---
sdk: ios
previous_release_tag: ios-0.9.0
---

# iOS SDK 1.0.0

## Highlights

<!-- AI-generated draft -->
`;

  const contentFile = `---
sdk: android
previous_release_tag: android-0.8.0
---

# Android SDK 1.0.0

## Highlights

Added new encryption support.
`;

  const noTagFile = `---
sdk: node
previous_release_tag: null
---

# Node SDK 1.0.0
`;

  const noSdkFile = `---
previous_release_tag: wasm-0.5.0
---

# Some SDK 1.0.0
`;

  it("classifies a mixed set of files", () => {
    const result = classifyNoteFiles([
      { path: "docs/release-notes/ios/1.0.0.md", content: emptyScaffold },
      {
        path: "docs/release-notes/android/1.0.0.md",
        content: contentFile,
      },
    ]);

    expect(result.empty).toEqual([
      {
        sdk: "ios",
        filePath: "docs/release-notes/ios/1.0.0.md",
        previousReleaseTag: "ios-0.9.0",
      },
    ]);
    expect(result.content).toEqual([
      {
        sdk: "android",
        filePath: "docs/release-notes/android/1.0.0.md",
        previousReleaseTag: "android-0.8.0",
      },
    ]);
  });

  it("skips files with missing previous_release_tag", () => {
    const result = classifyNoteFiles([
      { path: "docs/release-notes/node/1.0.0.md", content: noTagFile },
      { path: "docs/release-notes/ios/1.0.0.md", content: emptyScaffold },
    ]);

    expect(result.empty).toHaveLength(1);
    expect(result.content).toHaveLength(0);
    expect(result.empty[0].sdk).toBe("ios");
  });

  it("skips files with missing sdk", () => {
    const result = classifyNoteFiles([
      { path: "docs/release-notes/wasm/1.0.0.md", content: noSdkFile },
    ]);

    expect(result.empty).toHaveLength(0);
    expect(result.content).toHaveLength(0);
  });

  it("returns all empty when all are scaffolds", () => {
    const result = classifyNoteFiles([
      { path: "docs/release-notes/ios/1.0.0.md", content: emptyScaffold },
    ]);

    expect(result.empty).toHaveLength(1);
    expect(result.content).toHaveLength(0);
  });

  it("returns all content when all have content", () => {
    const result = classifyNoteFiles([
      {
        path: "docs/release-notes/android/1.0.0.md",
        content: contentFile,
      },
    ]);

    expect(result.empty).toHaveLength(0);
    expect(result.content).toHaveLength(1);
  });

  it("returns empty arrays when no files provided", () => {
    const result = classifyNoteFiles([]);

    expect(result.empty).toHaveLength(0);
    expect(result.content).toHaveLength(0);
  });
});
