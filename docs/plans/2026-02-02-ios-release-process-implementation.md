# iOS Release Process Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build release automation for the iOS SDK: a TypeScript CLI tool (`release-tools`) with tested library modules, conditional Package.swift logic, and GitHub Actions workflows for dev/rc/final releases.

**Architecture:** A TypeScript CLI in `dev/release-tools/` using yargs provides commands for version management, release notes scaffolding, and SPM updates. GitHub Actions orchestrator workflows dispatch to a reusable `release-ios.yml` that builds the xcframework via parallel matrix jobs, creates GitHub Releases, and publishes to CocoaPods and SPM.

**Tech Stack:** TypeScript, yargs, vitest, semver. GitHub Actions with Nix, `warp-macos-15-arm64-12x` runners.

**Design Document:** `docs/plans/2026-02-02-ios-release-process-design.md`

---

### Task 1: Initialize the TypeScript project

**Files:**
- Create: `dev/release-tools/package.json`
- Create: `dev/release-tools/tsconfig.json`
- Create: `dev/release-tools/vitest.config.ts`
- Create: `dev/release-tools/src/cli.ts`
- Create: `dev/release-tools/src/types.ts`

**Step 1: Create package.json**

```json
{
  "name": "@xmtp/release-tools",
  "version": "0.0.1",
  "private": true,
  "type": "module",
  "bin": {
    "release-tools": "./src/cli.ts"
  },
  "scripts": {
    "cli": "tsx src/cli.ts",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "semver": "^7.7.0",
    "yargs": "^17.7.0"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "@types/semver": "^7.5.0",
    "@types/yargs": "^17.0.0",
    "tsx": "^4.21.0",
    "typescript": "^5.9.0",
    "vitest": "^4.0.0"
  },
  "packageManager": "yarn@4.11.0",
  "engines": {
    "node": ">=22"
  }
}
```

**Step 2: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "rootDir": "src",
    "declaration": true,
    "sourceMap": true,
    "resolveJsonModule": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "tests"]
}
```

**Step 3: Create vitest.config.ts**

```typescript
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
  },
});
```

**Step 4: Create src/types.ts**

```typescript
export interface SdkConfig {
  /** Human-readable SDK name */
  name: string;
  /** Path to the version manifest file, relative to repo root */
  manifestPath: string;
  /** Path to SPM Package.swift, relative to repo root (optional, iOS-specific) */
  spmManifestPath?: string;
  /** Git tag prefix for this SDK */
  tagPrefix: string;
  /** Suffix for intermediate artifact tags */
  artifactTagSuffix: string;
}

export type ReleaseType = "dev" | "rc" | "final";

export type BumpType = "major" | "minor" | "patch";
```

**Step 5: Create src/cli.ts**

```typescript
#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
```

**Step 6: Install dependencies**

Run: `cd dev/release-tools && yarn install`

**Step 7: Verify CLI runs**

Run: `cd dev/release-tools && yarn cli --help`
Expected: Shows help text with "release-tools" and "You must specify a command"

**Step 8: Commit**

```bash
git add dev/release-tools/package.json dev/release-tools/tsconfig.json dev/release-tools/vitest.config.ts dev/release-tools/src/cli.ts dev/release-tools/src/types.ts dev/release-tools/yarn.lock
git commit -m "feat: initialize release-tools TypeScript project"
```

---

### Task 2: SDK config registry and version library

**Files:**
- Create: `dev/release-tools/src/lib/sdk-config.ts`
- Create: `dev/release-tools/src/lib/version.ts`
- Create: `dev/release-tools/tests/version.test.ts`

**Step 1: Write the failing tests for version utilities**

```typescript
// tests/version.test.ts
import { describe, it, expect } from "vitest";
import {
  computeVersion,
  filterAndSortTags,
} from "../src/lib/version.js";

describe("filterAndSortTags", () => {
  it("filters tags by prefix and sorts descending", () => {
    const tags = [
      "ios-4.8.0",
      "ios-4.9.0",
      "android-5.0.0",
      "ios-4.10.0",
      "ios-4.9.0-libxmtp",
    ];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual(["4.10.0", "4.9.0", "4.8.0"]);
  });

  it("includes prerelease tags when flag is set", () => {
    const tags = [
      "ios-4.9.0",
      "ios-4.10.0-rc.1",
      "ios-4.10.0-dev.abc1234",
    ];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp", true);
    expect(result).toEqual(["4.10.0-rc.1", "4.10.0-dev.abc1234", "4.9.0"]);
  });

  it("excludes prerelease tags by default", () => {
    const tags = [
      "ios-4.9.0",
      "ios-4.10.0-rc.1",
      "ios-4.10.0-dev.abc1234",
    ];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual(["4.9.0"]);
  });

  it("returns empty array when no tags match", () => {
    const tags = ["android-5.0.0", "kotlin-bindings-1.0.0"];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual([]);
  });

  it("excludes artifact tags ending in suffix", () => {
    const tags = [
      "ios-4.9.0",
      "ios-4.9.0-libxmtp",
      "ios-4.10.0-libxmtp",
    ];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp", true);
    expect(result).toEqual(["4.9.0"]);
  });
});

describe("computeVersion", () => {
  it("returns base version for final release", () => {
    expect(computeVersion("4.10.0", "final")).toBe("4.10.0");
  });

  it("appends rc suffix for rc release", () => {
    expect(computeVersion("4.10.0", "rc", { rcNumber: 1 })).toBe(
      "4.10.0-rc.1"
    );
  });

  it("appends dev suffix with short sha", () => {
    expect(
      computeVersion("4.10.0", "dev", { shortSha: "abc1234" })
    ).toBe("4.10.0-dev.abc1234");
  });

  it("throws if rc release has no rcNumber", () => {
    expect(() => computeVersion("4.10.0", "rc")).toThrow();
  });

  it("throws if dev release has no shortSha", () => {
    expect(() => computeVersion("4.10.0", "dev")).toThrow();
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - modules not found

**Step 3: Create sdk-config.ts**

```typescript
// src/lib/sdk-config.ts
import type { SdkConfig } from "../types.js";

export const SDK_CONFIGS: Record<string, SdkConfig> = {
  ios: {
    name: "iOS",
    manifestPath: "sdks/ios/XMTP.podspec",
    spmManifestPath: "Package.swift",
    tagPrefix: "ios-",
    artifactTagSuffix: "-libxmtp",
  },
};

export function getSdkConfig(sdk: string): SdkConfig {
  const config = SDK_CONFIGS[sdk];
  if (!config) {
    throw new Error(
      `Unknown SDK: ${sdk}. Available: ${Object.keys(SDK_CONFIGS).join(", ")}`
    );
  }
  return config;
}
```

**Step 4: Create version.ts**

```typescript
// src/lib/version.ts
import semver from "semver";
import type { ReleaseType } from "../types.js";

/**
 * Filter git tags by SDK prefix, exclude artifact tags, parse as semver,
 * and return sorted version strings (highest first).
 */
export function filterAndSortTags(
  tags: string[],
  prefix: string,
  artifactSuffix: string,
  includePrerelease = false
): string[] {
  const versions: semver.SemVer[] = [];

  for (const tag of tags) {
    if (!tag.startsWith(prefix)) continue;

    const versionStr = tag.slice(prefix.length);

    // Exclude artifact tags (e.g. ios-4.9.0-libxmtp)
    if (versionStr.endsWith(artifactSuffix)) continue;

    const parsed = semver.parse(versionStr);
    if (!parsed) continue;

    if (!includePrerelease && parsed.prerelease.length > 0) continue;

    versions.push(parsed);
  }

  return versions
    .sort((a, b) => semver.rcompare(a, b))
    .map((v) => v.version);
}

export interface ComputeVersionOptions {
  rcNumber?: number;
  shortSha?: string;
}

/**
 * Compute the full version string for a given release type.
 */
export function computeVersion(
  baseVersion: string,
  releaseType: ReleaseType,
  options: ComputeVersionOptions = {}
): string {
  switch (releaseType) {
    case "final":
      return baseVersion;
    case "rc": {
      if (options.rcNumber == null) {
        throw new Error("rcNumber is required for rc releases");
      }
      return `${baseVersion}-rc.${options.rcNumber}`;
    }
    case "dev": {
      if (!options.shortSha) {
        throw new Error("shortSha is required for dev releases");
      }
      return `${baseVersion}-dev.${options.shortSha}`;
    }
  }
}
```

**Step 5: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 6: Commit**

```bash
git add dev/release-tools/src/lib/sdk-config.ts dev/release-tools/src/lib/version.ts dev/release-tools/tests/version.test.ts
git commit -m "feat: add SDK config registry and version utilities with tests"
```

---

### Task 3: Podspec manifest reader/writer

**Files:**
- Create: `dev/release-tools/src/lib/manifest.ts`
- Create: `dev/release-tools/tests/manifest.test.ts`

The podspec format for the version line is: `  spec.version      = "4.9.0"`

**Step 1: Write the failing tests**

```typescript
// tests/manifest.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import {
  readPodspecVersion,
  writePodspecVersion,
} from "../src/lib/manifest.js";

const SAMPLE_PODSPEC = `Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0"

  spec.summary      = "XMTP SDK Cocoapod"
end
`;

describe("podspec manifest", () => {
  let tmpDir: string;
  let podspecPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-test-"));
    podspecPath = path.join(tmpDir, "XMTP.podspec");
    fs.writeFileSync(podspecPath, SAMPLE_PODSPEC);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("reads the version from a podspec", () => {
    expect(readPodspecVersion(podspecPath)).toBe("4.9.0");
  });

  it("writes a new version to a podspec", () => {
    writePodspecVersion(podspecPath, "4.10.0");
    expect(readPodspecVersion(podspecPath)).toBe("4.10.0");
  });

  it("preserves other content when writing", () => {
    writePodspecVersion(podspecPath, "5.0.0");
    const content = fs.readFileSync(podspecPath, "utf-8");
    expect(content).toContain('spec.name         = "XMTP"');
    expect(content).toContain('spec.version      = "5.0.0"');
    expect(content).toContain('spec.summary      = "XMTP SDK Cocoapod"');
  });

  it("throws if version line is not found", () => {
    fs.writeFileSync(podspecPath, "no version here\n");
    expect(() => readPodspecVersion(podspecPath)).toThrow();
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement manifest.ts**

```typescript
// src/lib/manifest.ts
import fs from "node:fs";

const PODSPEC_VERSION_REGEX = /(spec\.version\s*=\s*)"([^"]+)"/;

export function readPodspecVersion(podspecPath: string): string {
  const content = fs.readFileSync(podspecPath, "utf-8");
  const match = content.match(PODSPEC_VERSION_REGEX);
  if (!match) {
    throw new Error(
      `Could not find spec.version in ${podspecPath}`
    );
  }
  return match[2];
}

export function writePodspecVersion(
  podspecPath: string,
  version: string
): void {
  const content = fs.readFileSync(podspecPath, "utf-8");
  if (!PODSPEC_VERSION_REGEX.test(content)) {
    throw new Error(
      `Could not find spec.version in ${podspecPath}`
    );
  }
  const updated = content.replace(
    PODSPEC_VERSION_REGEX,
    `$1"${version}"`
  );
  fs.writeFileSync(podspecPath, updated);
}
```

**Step 4: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add dev/release-tools/src/lib/manifest.ts dev/release-tools/tests/manifest.test.ts
git commit -m "feat: add podspec manifest reader/writer with tests"
```

---

### Task 4: SPM Package.swift updater

**Files:**
- Create: `dev/release-tools/src/lib/spm.ts`
- Create: `dev/release-tools/tests/spm.test.ts`

This module updates the URL and checksum in the remote binary target branch of the conditional Package.swift. The Package.swift will have a structure like:

```swift
.binaryTarget(
    name: "LibXMTPSwiftFFI",
    url: "https://github.com/xmtp/libxmtp/releases/download/ios-4.9.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",
    checksum: "abc123..."
)
```

**Step 1: Write the failing tests**

```typescript
// tests/spm.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { updateSpmChecksum } from "../src/lib/spm.js";

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
      "newchecksum456"
    );
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain("ios-4.10.0-libxmtp");
    expect(content).toContain('checksum: "newchecksum456"');
    expect(content).not.toContain("oldchecksum123");
    expect(content).not.toContain("ios-4.9.0-libxmtp");
  });

  it("preserves the local binary target path", () => {
    updateSpmChecksum(
      packagePath,
      "https://example.com/new.zip",
      "abc"
    );
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain(
      'path: ".build/LibXMTPSwiftFFI.xcframework"'
    );
  });

  it("preserves the conditional logic", () => {
    updateSpmChecksum(
      packagePath,
      "https://example.com/new.zip",
      "abc"
    );
    const content = fs.readFileSync(packagePath, "utf-8");
    expect(content).toContain("useLocalBinary");
    expect(content).toContain("FileManager.default.fileExists");
  });

  it("throws if url pattern is not found", () => {
    fs.writeFileSync(packagePath, "no url here\n");
    expect(() =>
      updateSpmChecksum(packagePath, "https://example.com/new.zip", "abc")
    ).toThrow();
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement spm.ts**

```typescript
// src/lib/spm.ts
import fs from "node:fs";

// Matches the url line in a .binaryTarget declaration:
//   url: "https://...",
const SPM_URL_REGEX =
  /(\.binaryTarget\(\s*name:\s*"LibXMTPSwiftFFI",\s*url:\s*)"([^"]+)"/;

// Matches the checksum line:
//   checksum: "..."
const SPM_CHECKSUM_REGEX = /(checksum:\s*)"([^"]+)"/;

export function updateSpmChecksum(
  packageSwiftPath: string,
  url: string,
  checksum: string
): void {
  let content = fs.readFileSync(packageSwiftPath, "utf-8");

  if (!SPM_URL_REGEX.test(content)) {
    throw new Error(
      `Could not find remote binaryTarget url in ${packageSwiftPath}`
    );
  }

  content = content.replace(SPM_URL_REGEX, `$1"${url}"`);
  content = content.replace(SPM_CHECKSUM_REGEX, `$1"${checksum}"`);

  fs.writeFileSync(packageSwiftPath, content);
}
```

**Step 4: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add dev/release-tools/src/lib/spm.ts dev/release-tools/tests/spm.test.ts
git commit -m "feat: add SPM Package.swift url/checksum updater with tests"
```

---

### Task 5: Git helpers

**Files:**
- Create: `dev/release-tools/src/lib/git.ts`
- Create: `dev/release-tools/tests/git.test.ts`

These wrap `git` commands via `child_process.execSync`. Tests use a temporary git repo.

**Step 1: Write the failing tests**

```typescript
// tests/git.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { listTags, getShortSha, getCommitsBetween } from "../src/lib/git.js";

describe("git helpers", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-tools-git-"));
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial commit'", { cwd: tmpDir });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  describe("listTags", () => {
    it("returns all tags", () => {
      execSync("git tag ios-4.8.0", { cwd: tmpDir });
      execSync("git tag ios-4.9.0", { cwd: tmpDir });
      execSync("git tag android-1.0.0", { cwd: tmpDir });
      const tags = listTags(tmpDir);
      expect(tags).toContain("ios-4.8.0");
      expect(tags).toContain("ios-4.9.0");
      expect(tags).toContain("android-1.0.0");
    });

    it("returns empty array for repo with no tags", () => {
      expect(listTags(tmpDir)).toEqual([]);
    });
  });

  describe("getShortSha", () => {
    it("returns a 7-character hash", () => {
      const sha = getShortSha(tmpDir);
      expect(sha).toMatch(/^[0-9a-f]{7}$/);
    });
  });

  describe("getCommitsBetween", () => {
    it("returns commits between two refs", () => {
      execSync("git tag v1", { cwd: tmpDir });
      fs.writeFileSync(path.join(tmpDir, "file.txt"), "change1");
      execSync("git add . && git commit -m 'feat: add feature'", {
        cwd: tmpDir,
      });
      fs.writeFileSync(path.join(tmpDir, "file.txt"), "change2");
      execSync("git add . && git commit -m 'fix: bug fix'", {
        cwd: tmpDir,
      });
      const commits = getCommitsBetween(tmpDir, "v1", "HEAD");
      expect(commits).toHaveLength(2);
      expect(commits[0]).toContain("fix: bug fix");
      expect(commits[1]).toContain("feat: add feature");
    });

    it("returns all commits from HEAD when sinceRef is null", () => {
      const commits = getCommitsBetween(tmpDir, null, "HEAD");
      expect(commits.length).toBeGreaterThan(0);
    });
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement git.ts**

```typescript
// src/lib/git.ts
import { execSync } from "node:child_process";

function exec(cmd: string, cwd: string): string {
  return execSync(cmd, { cwd, encoding: "utf-8" }).trim();
}

export function listTags(cwd: string): string[] {
  const output = exec("git tag --list", cwd);
  if (!output) return [];
  return output.split("\n").filter(Boolean);
}

export function getShortSha(cwd: string, ref = "HEAD"): string {
  return exec(`git rev-parse --short=7 ${ref}`, cwd);
}

export function getCommitsBetween(
  cwd: string,
  sinceRef: string | null,
  untilRef: string
): string[] {
  const range = sinceRef ? `${sinceRef}..${untilRef}` : untilRef;
  const output = exec(
    `git log ${range} --oneline --no-decorate`,
    cwd
  );
  if (!output) return [];
  return output.split("\n").filter(Boolean);
}
```

**Step 4: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add dev/release-tools/src/lib/git.ts dev/release-tools/tests/git.test.ts
git commit -m "feat: add git helper utilities with tests"
```

---

### Task 6: `find-last-version` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/find-last-version.ts`
- Create: `dev/release-tools/tests/commands/find-last-version.test.ts`
- Modify: `dev/release-tools/src/cli.ts`

**Step 1: Write the failing test**

```typescript
// tests/commands/find-last-version.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { findLastVersion } from "../../src/commands/find-last-version.js";

describe("findLastVersion", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-flv-")
    );
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial'", { cwd: tmpDir });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("finds the latest stable ios version", () => {
    execSync("git tag ios-4.8.0", { cwd: tmpDir });
    execSync("git tag ios-4.9.0", { cwd: tmpDir });
    execSync("git tag ios-4.9.0-libxmtp", { cwd: tmpDir });
    expect(findLastVersion("ios", tmpDir)).toBe("4.9.0");
  });

  it("returns null when no tags exist", () => {
    expect(findLastVersion("ios", tmpDir)).toBeNull();
  });

  it("skips prerelease by default", () => {
    execSync("git tag ios-4.9.0", { cwd: tmpDir });
    execSync("git tag ios-4.10.0-rc.1", { cwd: tmpDir });
    expect(findLastVersion("ios", tmpDir)).toBe("4.9.0");
  });

  it("includes prerelease when requested", () => {
    execSync("git tag ios-4.9.0", { cwd: tmpDir });
    execSync("git tag ios-4.10.0-rc.1", { cwd: tmpDir });
    expect(findLastVersion("ios", tmpDir, true)).toBe("4.10.0-rc.1");
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement find-last-version.ts**

```typescript
// src/commands/find-last-version.ts
import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { filterAndSortTags } from "../lib/version.js";
import { listTags } from "../lib/git.js";

export function findLastVersion(
  sdk: string,
  cwd: string,
  preRelease = false
): string | null {
  const config = getSdkConfig(sdk);
  const tags = listTags(cwd);
  const versions = filterAndSortTags(
    tags,
    config.tagPrefix,
    config.artifactTagSuffix,
    preRelease
  );
  return versions.length > 0 ? versions[0] : null;
}

export const command = "find-last-version";
export const describe = "Find the latest published version for an SDK";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("pre-release", {
      type: "boolean",
      default: false,
      describe: "Include prerelease versions",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{ sdk: string; preRelease: boolean }>
) {
  const version = findLastVersion(
    argv.sdk,
    process.cwd(),
    argv.preRelease
  );
  if (version) {
    console.log(version);
  } else {
    console.log("");
  }
}
```

**Step 4: Register command in cli.ts**

Update `src/cli.ts`:

```typescript
#!/usr/bin/env tsx
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import * as findLastVersion from "./commands/find-last-version.js";

yargs(hideBin(process.argv))
  .scriptName("release-tools")
  .command(findLastVersion)
  .demandCommand(1, "You must specify a command")
  .strict()
  .help()
  .parse();
```

**Step 5: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 6: Verify CLI works**

Run: `cd dev/release-tools && yarn cli find-last-version --sdk ios`
Expected: Outputs the latest iOS version tag from the repo (or empty string if none)

**Step 7: Commit**

```bash
git add dev/release-tools/src/commands/find-last-version.ts dev/release-tools/tests/commands/find-last-version.test.ts dev/release-tools/src/cli.ts
git commit -m "feat: add find-last-version command"
```

---

### Task 7: `bump-version` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/bump-version.ts`
- Create: `dev/release-tools/tests/commands/bump-version.test.ts`
- Modify: `dev/release-tools/src/cli.ts`

**Step 1: Write the failing test**

```typescript
// tests/commands/bump-version.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { bumpVersion } from "../../src/commands/bump-version.js";
import { readPodspecVersion } from "../../src/lib/manifest.js";

const SAMPLE_PODSPEC = `Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0"
end
`;

describe("bumpVersion", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-bump-")
    );
    const podspecDir = path.join(tmpDir, "sdks", "ios");
    fs.mkdirSync(podspecDir, { recursive: true });
    fs.writeFileSync(
      path.join(podspecDir, "XMTP.podspec"),
      SAMPLE_PODSPEC
    );
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("bumps patch version", () => {
    const result = bumpVersion("ios", "patch", tmpDir);
    expect(result).toBe("4.9.1");
    expect(
      readPodspecVersion(path.join(tmpDir, "sdks/ios/XMTP.podspec"))
    ).toBe("4.9.1");
  });

  it("bumps minor version", () => {
    const result = bumpVersion("ios", "minor", tmpDir);
    expect(result).toBe("4.10.0");
  });

  it("bumps major version", () => {
    const result = bumpVersion("ios", "major", tmpDir);
    expect(result).toBe("5.0.0");
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement bump-version.ts**

```typescript
// src/commands/bump-version.ts
import path from "node:path";
import semver from "semver";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";
import { readPodspecVersion, writePodspecVersion } from "../lib/manifest.js";

export function bumpVersion(
  sdk: string,
  bumpType: BumpType,
  repoRoot: string
): string {
  const config = getSdkConfig(sdk);
  const manifestPath = path.join(repoRoot, config.manifestPath);
  const currentVersion = readPodspecVersion(manifestPath);
  const newVersion = semver.inc(currentVersion, bumpType);
  if (!newVersion) {
    throw new Error(
      `Failed to bump ${bumpType} on version ${currentVersion}`
    );
  }
  writePodspecVersion(manifestPath, newVersion);
  return newVersion;
}

export const command = "bump-version";
export const describe = "Bump the version in an SDK manifest";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("type", {
      type: "string",
      demandOption: true,
      choices: ["major", "minor", "patch"] as const,
      describe: "Version bump type",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{ sdk: string; type: BumpType }>
) {
  const version = bumpVersion(argv.sdk, argv.type, process.cwd());
  console.log(version);
}
```

**Step 4: Register in cli.ts**

Add `import * as bumpVersion from "./commands/bump-version.js";` and `.command(bumpVersion)` to cli.ts.

**Step 5: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 6: Commit**

```bash
git add dev/release-tools/src/commands/bump-version.ts dev/release-tools/tests/commands/bump-version.test.ts dev/release-tools/src/cli.ts
git commit -m "feat: add bump-version command"
```

---

### Task 8: `compute-version` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/compute-version.ts`
- Modify: `dev/release-tools/src/cli.ts`

No additional tests needed - the core logic is already tested in `tests/version.test.ts`. This task wires the library function to a CLI command.

**Step 1: Implement compute-version.ts**

```typescript
// src/commands/compute-version.ts
import path from "node:path";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { ReleaseType } from "../types.js";
import { getSdkConfig } from "../lib/sdk-config.js";
import { readPodspecVersion } from "../lib/manifest.js";
import { computeVersion as computeVersionFn } from "../lib/version.js";
import { getShortSha } from "../lib/git.js";

export const command = "compute-version";
export const describe =
  "Compute the full version string for a release type";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("release-type", {
      type: "string",
      demandOption: true,
      choices: ["dev", "rc", "final"] as const,
      describe: "Release type",
    })
    .option("rc-number", {
      type: "number",
      describe: "RC number (required for rc releases)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{
    sdk: string;
    releaseType: ReleaseType;
    rcNumber?: number;
  }>
) {
  const config = getSdkConfig(argv.sdk);
  const manifestPath = path.join(process.cwd(), config.manifestPath);
  const baseVersion = readPodspecVersion(manifestPath);
  const shortSha =
    argv.releaseType === "dev" ? getShortSha(process.cwd()) : undefined;
  const version = computeVersionFn(baseVersion, argv.releaseType, {
    rcNumber: argv.rcNumber,
    shortSha,
  });
  console.log(version);
}
```

**Step 2: Register in cli.ts**

Add `import * as computeVersion from "./commands/compute-version.js";` and `.command(computeVersion)`.

**Step 3: Verify it works**

Run: `cd dev/release-tools && yarn cli compute-version --sdk ios --release-type final`
Expected: `4.9.0`

**Step 4: Commit**

```bash
git add dev/release-tools/src/commands/compute-version.ts dev/release-tools/src/cli.ts
git commit -m "feat: add compute-version command"
```

---

### Task 9: `update-spm-checksum` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/update-spm-checksum.ts`
- Modify: `dev/release-tools/src/cli.ts`

No additional tests - core logic is tested in `tests/spm.test.ts`.

**Step 1: Implement update-spm-checksum.ts**

```typescript
// src/commands/update-spm-checksum.ts
import path from "node:path";
import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { updateSpmChecksum as updateSpmChecksumFn } from "../lib/spm.js";

export const command = "update-spm-checksum";
export const describe =
  "Update the binary target URL and checksum in Package.swift";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("url", {
      type: "string",
      demandOption: true,
      describe: "Artifact download URL",
    })
    .option("checksum", {
      type: "string",
      demandOption: true,
      describe: "SHA-256 checksum of the artifact",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{
    sdk: string;
    url: string;
    checksum: string;
  }>
) {
  const config = getSdkConfig(argv.sdk);
  if (!config.spmManifestPath) {
    throw new Error(`SDK ${argv.sdk} does not have an SPM manifest`);
  }
  const spmPath = path.join(process.cwd(), config.spmManifestPath);
  updateSpmChecksumFn(spmPath, argv.url, argv.checksum);
  console.log(`Updated ${config.spmManifestPath}`);
}
```

**Step 2: Register in cli.ts**

Add `import * as updateSpmChecksum from "./commands/update-spm-checksum.js";` and `.command(updateSpmChecksum)`.

**Step 3: Run all tests**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add dev/release-tools/src/commands/update-spm-checksum.ts dev/release-tools/src/cli.ts
git commit -m "feat: add update-spm-checksum command"
```

---

### Task 10: `scaffold-notes` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/scaffold-notes.ts`
- Create: `dev/release-tools/tests/commands/scaffold-notes.test.ts`
- Modify: `dev/release-tools/src/cli.ts`

**Step 1: Write the failing tests**

```typescript
// tests/commands/scaffold-notes.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { scaffoldNotes } from "../../src/commands/scaffold-notes.js";

describe("scaffoldNotes", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "release-tools-notes-")
    );
    execSync("git init", { cwd: tmpDir });
    execSync("git config user.email test@test.com", { cwd: tmpDir });
    execSync("git config user.name Test", { cwd: tmpDir });
    // Create directory structure
    fs.mkdirSync(path.join(tmpDir, "sdks/ios"), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, "docs/release-notes"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(tmpDir, "sdks/ios/XMTP.podspec"),
      `Pod::Spec.new do |spec|\n  spec.version      = "4.10.0"\nend\n`
    );
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "initial");
    execSync("git add . && git commit -m 'initial commit'", {
      cwd: tmpDir,
    });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("creates release notes from a since tag", () => {
    execSync("git tag ios-4.9.0", { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "change1");
    execSync("git add . && git commit -m 'feat: new feature'", {
      cwd: tmpDir,
    });
    fs.writeFileSync(path.join(tmpDir, "file.txt"), "change2");
    execSync("git add . && git commit -m 'fix: bug fix'", {
      cwd: tmpDir,
    });

    const outputPath = scaffoldNotes("ios", tmpDir, "ios-4.9.0");
    expect(outputPath).toContain("docs/release-notes/ios-4.10.0.md");

    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain("new feature");
    expect(content).toContain("bug fix");
  });

  it("handles no previous tag gracefully", () => {
    const outputPath = scaffoldNotes("ios", tmpDir, null);
    const content = fs.readFileSync(outputPath, "utf-8");
    expect(content).toContain("# iOS SDK 4.10.0");
    expect(content).toContain("first release from the monorepo");
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd dev/release-tools && yarn test`
Expected: FAIL - module not found

**Step 3: Implement scaffold-notes.ts**

```typescript
// src/commands/scaffold-notes.ts
import path from "node:path";
import fs from "node:fs";
import type { ArgumentsCamelCase, Argv } from "yargs";
import { getSdkConfig } from "../lib/sdk-config.js";
import { readPodspecVersion } from "../lib/manifest.js";
import { getCommitsBetween } from "../lib/git.js";
import { findLastVersion } from "./find-last-version.js";

export function scaffoldNotes(
  sdk: string,
  repoRoot: string,
  sinceTag: string | null
): string {
  const config = getSdkConfig(sdk);
  const manifestPath = path.join(repoRoot, config.manifestPath);
  const version = readPodspecVersion(manifestPath);

  const notesDir = path.join(repoRoot, "docs/release-notes");
  fs.mkdirSync(notesDir, { recursive: true });
  const outputPath = path.join(
    notesDir,
    `${config.tagPrefix.replace(/-$/, "")}-${version}.md`
  );

  let commitSection: string;
  if (sinceTag) {
    const commits = getCommitsBetween(repoRoot, sinceTag, "HEAD");
    commitSection = commits.length > 0
      ? commits.map((c) => `- ${c}`).join("\n")
      : "- No changes since last release";
  } else {
    commitSection =
      "This is the first release from the monorepo. No previous release tag was found to compare against.";
  }

  const content = `# ${config.name} SDK ${version}

## Highlights

<!-- AI-generated draft - please review and edit -->

## What's Changed

${commitSection}

## Breaking Changes

<!-- List any breaking changes here -->

## New Features

<!-- List new features here -->

## Bug Fixes

<!-- List bug fixes here -->

## Dependencies

<!-- Note any dependency changes -->
`;

  fs.writeFileSync(outputPath, content);
  return outputPath;
}

export const command = "scaffold-notes";
export const describe = "Generate a release notes template from git history";

export function builder(yargs: Argv) {
  return yargs
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK name (e.g. ios)",
    })
    .option("since", {
      type: "string",
      describe:
        "Tag to diff from (defaults to last stable release tag)",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{ sdk: string; since?: string }>
) {
  const sinceTag =
    argv.since ??
    (() => {
      const lastVersion = findLastVersion(argv.sdk, process.cwd());
      if (!lastVersion) return null;
      const config = getSdkConfig(argv.sdk);
      return `${config.tagPrefix}${lastVersion}`;
    })();

  const outputPath = scaffoldNotes(argv.sdk, process.cwd(), sinceTag);
  console.log(outputPath);
}
```

**Step 4: Run tests to verify they pass**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 5: Register in cli.ts**

Add `import * as scaffoldNotes from "./commands/scaffold-notes.js";` and `.command(scaffoldNotes)`.

**Step 6: Commit**

```bash
git add dev/release-tools/src/commands/scaffold-notes.ts dev/release-tools/tests/commands/scaffold-notes.test.ts dev/release-tools/src/cli.ts
git commit -m "feat: add scaffold-notes command"
```

---

### Task 11: `create-release-branch` CLI command

**Files:**
- Create: `dev/release-tools/src/commands/create-release-branch.ts`
- Modify: `dev/release-tools/src/cli.ts`

This command orchestrates branch creation, version bumping, and notes scaffolding. It calls git directly and uses the other commands. Integration testing is done via the CLI; the component parts are already unit-tested.

**Step 1: Implement create-release-branch.ts**

```typescript
// src/commands/create-release-branch.ts
import { execSync } from "node:child_process";
import type { ArgumentsCamelCase, Argv } from "yargs";
import type { BumpType } from "../types.js";
import { bumpVersion } from "./bump-version.js";
import { scaffoldNotes } from "./scaffold-notes.js";
import { findLastVersion } from "./find-last-version.js";
import { getSdkConfig } from "../lib/sdk-config.js";

function exec(cmd: string, cwd: string): void {
  execSync(cmd, { cwd, stdio: "inherit" });
}

export const command = "create-release-branch";
export const describe =
  "Create a release branch with bumped versions and scaffolded notes";

export function builder(yargs: Argv) {
  return yargs
    .option("version", {
      type: "string",
      demandOption: true,
      describe: "Release version number (used in branch name)",
    })
    .option("base", {
      type: "string",
      default: "HEAD",
      describe: "Base ref to branch from",
    })
    .option("sdk", {
      type: "string",
      demandOption: true,
      describe: "SDK to bump (e.g. ios)",
    })
    .option("bump", {
      type: "string",
      demandOption: true,
      choices: ["major", "minor", "patch"] as const,
      describe: "Version bump type",
    });
}

export function handler(
  argv: ArgumentsCamelCase<{
    version: string;
    base: string;
    sdk: string;
    bump: BumpType;
  }>
) {
  const cwd = process.cwd();
  const branchName = `release/${argv.version}`;

  console.log(`Creating branch ${branchName} from ${argv.base}...`);
  exec(`git checkout -b ${branchName} ${argv.base}`, cwd);

  console.log(`Bumping ${argv.sdk} version (${argv.bump})...`);
  const newVersion = bumpVersion(argv.sdk, argv.bump, cwd);
  console.log(`New ${argv.sdk} version: ${newVersion}`);

  const config = getSdkConfig(argv.sdk);
  const lastVersion = findLastVersion(argv.sdk, cwd);
  const sinceTag = lastVersion
    ? `${config.tagPrefix}${lastVersion}`
    : null;
  console.log(`Scaffolding release notes...`);
  const notesPath = scaffoldNotes(argv.sdk, cwd, sinceTag);
  console.log(`Release notes: ${notesPath}`);

  exec("git add -A", cwd);
  exec(
    `git commit -m "chore: create release ${argv.version} with ${argv.sdk} ${newVersion}"`,
    cwd
  );

  console.log(`Branch ${branchName} created and committed.`);
  console.log(`Push with: git push -u origin ${branchName}`);
}
```

**Step 2: Register in cli.ts**

Add `import * as createReleaseBranch from "./commands/create-release-branch.js";` and `.command(createReleaseBranch)`.

**Step 3: Run all tests**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add dev/release-tools/src/commands/create-release-branch.ts dev/release-tools/src/cli.ts
git commit -m "feat: add create-release-branch command"
```

---

### Task 12: ~~Update Package.swift with conditional binary target~~ DONE

> **Note:** Package.swift has been moved to the repo root (see `docs/plans/2026-02-02-move-package-swift-to-root.md`). It now lives at `Package.swift` with all target paths prefixed with `sdks/ios/` and the local binary target pointing to `sdks/ios/.build/LibXMTPSwiftFFI.xcframework`. The conditional logic and placeholder URL/checksum are already in place.

---

### Task 13: Reusable `release-ios.yml` workflow

**Files:**
- Create: `.github/workflows/release-ios.yml`

**Step 1: Create the reusable workflow**

```yaml
# .github/workflows/release-ios.yml
name: Release iOS SDK

on:
  workflow_call:
    inputs:
      release-type:
        required: true
        type: string
        description: "dev, rc, or final"
      rc-number:
        required: false
        type: number
        description: "RC number (required for rc releases)"
      ref:
        required: true
        type: string
        description: "Git ref to build from"
    outputs:
      version:
        description: "The published version string"
        value: ${{ jobs.publish.outputs.version }}

jobs:
  compute-version:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
      base-version: ${{ steps.version.outputs.base-version }}
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
          fetch-depth: 0
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
      - name: Install release tools
        working-directory: dev/release-tools
        run: yarn install --frozen-lockfile
      - name: Compute version
        id: version
        working-directory: dev/release-tools
        run: |
          BASE=$(yarn cli compute-version --sdk ios --release-type final)
          if [ "${{ inputs.release-type }}" = "rc" ]; then
            VERSION=$(yarn cli compute-version --sdk ios --release-type rc --rc-number ${{ inputs.rc-number }})
          elif [ "${{ inputs.release-type }}" = "dev" ]; then
            VERSION=$(yarn cli compute-version --sdk ios --release-type dev)
          else
            VERSION=$BASE
          fi
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"
          echo "base-version=$BASE" >> "$GITHUB_OUTPUT"
          echo "Computed version: $VERSION"

  build:
    needs: [compute-version]
    runs-on: warp-macos-15-arm64-12x
    strategy:
      fail-fast: false
      matrix:
        target:
          - aarch64-apple-ios
          - aarch64-apple-ios-sim
          - x86_64-apple-darwin
          - aarch64-apple-darwin
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            .
      - name: Build target
        run: |
          nix develop --command \
            cargo build --release --target ${{ matrix.target }} --manifest-path bindings/mobile/Cargo.toml
      - uses: actions/upload-artifact@v6
        with:
          name: ${{ matrix.target }}
          path: target/${{ matrix.target }}/release/libxmtpv3.a
          retention-days: 1

  generate-swift-bindings:
    runs-on: warp-macos-15-arm64-12x
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            .
      - name: Generate Swift bindings
        working-directory: bindings/mobile
        run: nix develop ../.. --command make swift
      - uses: actions/upload-artifact@v6
        with:
          name: swift
          path: bindings/mobile/build/swift/
          retention-days: 1

  package:
    needs: [compute-version, build, generate-swift-bindings]
    runs-on: warp-macos-15-arm64-12x
    permissions:
      contents: write
    outputs:
      artifact-url: ${{ steps.release.outputs.artifact-url }}
      checksum: ${{ steps.checksum.outputs.checksum }}
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
      - uses: actions/download-artifact@v7
        with:
          path: bindings/mobile/build
      - name: Build xcframework
        working-directory: bindings/mobile
        run: |
          mkdir -p Sources/LibXMTP
          mv build/swift/xmtpv3.swift Sources/LibXMTP/
          make framework
          cp ../../LICENSE ./LICENSE
          zip -r LibXMTPSwiftFFI.zip Sources LibXMTPSwiftFFI.xcframework LICENSE
      - name: Compute checksum
        id: checksum
        working-directory: bindings/mobile
        run: |
          echo "checksum=$(shasum -a 256 LibXMTPSwiftFFI.zip | awk '{ print $1 }')" >> "$GITHUB_OUTPUT"
      - name: Create or update GitHub Release
        id: release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VERSION: ${{ needs.compute-version.outputs.version }}
        working-directory: bindings/mobile
        run: |
          TAG="ios-${VERSION}-libxmtp"
          # Delete existing release if re-running
          gh release delete "$TAG" --yes 2>/dev/null || true
          git tag -d "$TAG" 2>/dev/null || true
          git push origin ":refs/tags/$TAG" 2>/dev/null || true

          gh release create "$TAG" \
            --title "iOS $VERSION - libxmtp binaries" \
            --notes "Intermediate artifact release for iOS SDK $VERSION" \
            --prerelease \
            LibXMTPSwiftFFI.zip

          ARTIFACT_URL="https://github.com/${{ github.repository }}/releases/download/${TAG}/LibXMTPSwiftFFI.zip"
          echo "artifact-url=$ARTIFACT_URL" >> "$GITHUB_OUTPUT"

  publish:
    needs: [compute-version, package]
    runs-on: warp-macos-15-arm64-12x
    permissions:
      contents: write
    outputs:
      version: ${{ needs.compute-version.outputs.version }}
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
          fetch-depth: 0
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
      - name: Install release tools
        working-directory: dev/release-tools
        run: yarn install --frozen-lockfile
      - name: Update Package.swift and podspec
        env:
          VERSION: ${{ needs.compute-version.outputs.version }}
          ARTIFACT_URL: ${{ needs.package.outputs.artifact-url }}
          CHECKSUM: ${{ needs.package.outputs.checksum }}
        run: |
          cd dev/release-tools
          yarn cli update-spm-checksum --sdk ios --url "$ARTIFACT_URL" --checksum "$CHECKSUM"

          # For dev/rc releases, update podspec with suffixed version
          if [ "${{ inputs.release-type }}" != "final" ]; then
            # Write the full version (with suffix) directly to podspec
            sed -i '' "s/spec.version.*=.*/spec.version      = \"$VERSION\"/" sdks/ios/XMTP.podspec
          fi
      - name: Commit and tag
        env:
          VERSION: ${{ needs.compute-version.outputs.version }}
        run: |
          TAG="ios-${VERSION}"
          # Clean up existing tag if re-running
          git tag -d "$TAG" 2>/dev/null || true
          git push origin ":refs/tags/$TAG" 2>/dev/null || true

          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Package.swift sdks/ios/XMTP.podspec
          git commit -m "release: iOS SDK $VERSION [skip ci]" || echo "No changes to commit"
          git tag "$TAG"
          git push origin HEAD "$TAG"
      - name: Copy release notes (final only)
        if: inputs.release-type == 'final'
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VERSION: ${{ needs.compute-version.outputs.version }}
        run: |
          TAG="ios-${VERSION}"
          NOTES_FILE="docs/release-notes/ios-${VERSION}.md"
          if [ -f "$NOTES_FILE" ]; then
            gh release create "$TAG" \
              --title "iOS SDK $VERSION" \
              --notes-file "$NOTES_FILE" \
              --latest
          else
            gh release create "$TAG" \
              --title "iOS SDK $VERSION" \
              --notes "iOS SDK version $VERSION" \
              --latest
          fi
      - name: Publish to CocoaPods
        env:
          COCOAPODS_TRUNK_TOKEN: ${{ secrets.COCOAPODS_TRUNK_TOKEN }}
        run: |
          # Check if version is already published
          PUBLISHED=$(pod trunk info XMTP 2>/dev/null | grep -c "$VERSION" || true)
          if [ "$PUBLISHED" -gt 0 ]; then
            echo "Version $VERSION already published to CocoaPods, skipping"
          else
            pod trunk push sdks/ios/XMTP.podspec --allow-warnings --skip-tests
          fi
```

**Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release-ios.yml'))"`
Expected: No errors

**Step 3: Commit**

```bash
git add .github/workflows/release-ios.yml
git commit -m "feat: add reusable release-ios workflow with parallel matrix builds"
```

---

### Task 14: `dev-release.yml` orchestrator workflow

**Files:**
- Create: `.github/workflows/dev-release.yml`

**Step 1: Create the workflow**

```yaml
# .github/workflows/dev-release.yml
name: Dev Release

on:
  workflow_dispatch:
    inputs:
      branch:
        description: "Branch to release from"
        required: true
        type: string
      ios:
        description: "Release iOS SDK"
        required: false
        type: boolean
        default: false

jobs:
  release-ios:
    if: inputs.ios
    uses: ./.github/workflows/release-ios.yml
    with:
      release-type: dev
      ref: ${{ inputs.branch }}
    secrets: inherit

  notify:
    needs: [release-ios]
    if: always()
    runs-on: ubuntu-latest
    steps:
      - name: Notify Slack
        env:
          SLACK_WEBHOOK_URL: ${{ secrets.SLACK_WEBHOOK_URL }}
          IOS_RESULT: ${{ needs.release-ios.result }}
          IOS_VERSION: ${{ needs.release-ios.outputs.version }}
        run: |
          STATUS="success"
          if [ "$IOS_RESULT" = "failure" ]; then
            STATUS="failure"
          fi

          CHANNEL="#notify-dev-releases"
          if [ "$STATUS" = "failure" ]; then
            CHANNEL="#notify-dev-release-failures"
          fi

          MESSAGE="Dev Release: iOS ${IOS_VERSION:-skipped} - ${IOS_RESULT:-skipped}"

          if [ -n "$SLACK_WEBHOOK_URL" ]; then
            curl -X POST "$SLACK_WEBHOOK_URL" \
              -H 'Content-type: application/json' \
              --data "{\"channel\":\"$CHANNEL\",\"text\":\"$MESSAGE\"}"
          else
            echo "$MESSAGE"
          fi
```

**Step 2: Commit**

```bash
git add .github/workflows/dev-release.yml
git commit -m "feat: add dev-release orchestrator workflow"
```

---

### Task 15: `create-release-branch.yml` orchestrator workflow

**Files:**
- Create: `.github/workflows/create-release-branch.yml`

**Step 1: Create the workflow**

```yaml
# .github/workflows/create-release-branch.yml
name: Create Release Branch

on:
  workflow_dispatch:
    inputs:
      base-ref:
        description: "Base ref (commit/branch) to create the release from"
        required: true
        type: string
        default: "main"
      version:
        description: "Release version number (e.g. 1.8.0)"
        required: true
        type: string
      ios-bump:
        description: "iOS SDK version bump"
        required: false
        type: choice
        options:
          - "none"
          - "patch"
          - "minor"
          - "major"
        default: "none"

jobs:
  create-branch:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.base-ref }}
          fetch-depth: 0
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
      - name: Install release tools
        working-directory: dev/release-tools
        run: yarn install --frozen-lockfile
      - name: Create release branch
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

          BRANCH="release/${{ inputs.version }}"
          git checkout -b "$BRANCH"

          if [ "${{ inputs.ios-bump }}" != "none" ]; then
            cd dev/release-tools
            NEW_VERSION=$(yarn cli bump-version --sdk ios --type ${{ inputs.ios-bump }})
            echo "iOS version bumped to: $NEW_VERSION"
            yarn cli scaffold-notes --sdk ios
            cd ../..
          fi

          git add -A
          git commit -m "chore: create release ${{ inputs.version }}" || echo "No changes"
          git push -u origin "$BRANCH"

  draft-release-notes:
    needs: [create-branch]
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - name: Trigger Claude Code for release notes
        uses: anthropics/claude-code-action@v1
        with:
          prompt: |
            Review the draft release notes in docs/release-notes/ on the release/${{ inputs.version }} branch.
            Improve them based on the actual code changes. Open a PR targeting the release branch with your improvements.
```

**Step 2: Commit**

```bash
git add .github/workflows/create-release-branch.yml
git commit -m "feat: add create-release-branch orchestrator workflow"
```

---

### Task 16: `publish-release.yml` orchestrator workflow

**Files:**
- Create: `.github/workflows/publish-release.yml`

**Step 1: Create the workflow**

```yaml
# .github/workflows/publish-release.yml
name: Publish Release

on:
  workflow_dispatch:
    inputs:
      release-branch:
        description: "Release or hotfix branch to publish from"
        required: true
        type: string
      release-type:
        description: "Release type"
        required: true
        type: choice
        options:
          - "rc"
          - "final"
      rc-number:
        description: "RC number (required for RC releases)"
        required: false
        type: number
      ios:
        description: "Release iOS SDK"
        required: false
        type: boolean
        default: false

jobs:
  release-ios:
    if: inputs.ios
    uses: ./.github/workflows/release-ios.yml
    with:
      release-type: ${{ inputs.release-type }}
      rc-number: ${{ inputs.rc-number }}
      ref: ${{ inputs.release-branch }}
    secrets: inherit

  merge-to-main:
    if: inputs.release-type == 'final' && !failure() && !cancelled()
    needs: [release-ios]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v6
        with:
          ref: main
          fetch-depth: 0
      - name: Merge release branch
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git fetch origin ${{ inputs.release-branch }}
          git merge origin/${{ inputs.release-branch }} --no-ff -m "chore: merge release branch ${{ inputs.release-branch }}"
          git push origin main

  notify:
    needs: [release-ios, merge-to-main]
    if: always()
    runs-on: ubuntu-latest
    steps:
      - name: Notify Slack
        env:
          SLACK_WEBHOOK_URL: ${{ secrets.SLACK_WEBHOOK_URL }}
          IOS_RESULT: ${{ needs.release-ios.result }}
          IOS_VERSION: ${{ needs.release-ios.outputs.version }}
          RELEASE_TYPE: ${{ inputs.release-type }}
        run: |
          STATUS="success"
          if [ "$IOS_RESULT" = "failure" ]; then
            STATUS="failure"
          fi

          if [ "$RELEASE_TYPE" = "final" ]; then
            CHANNEL="#notify-sdk-releases"
          else
            CHANNEL="#notify-dev-releases"
          fi

          if [ "$STATUS" = "failure" ]; then
            CHANNEL="#notify-dev-release-failures"
          fi

          MESSAGE="${RELEASE_TYPE^} Release: iOS ${IOS_VERSION:-skipped} - ${IOS_RESULT:-skipped}"

          if [ -n "$SLACK_WEBHOOK_URL" ]; then
            curl -X POST "$SLACK_WEBHOOK_URL" \
              -H 'Content-type: application/json' \
              --data "{\"channel\":\"$CHANNEL\",\"text\":\"$MESSAGE\"}"
          else
            echo "$MESSAGE"
          fi
```

**Step 2: Commit**

```bash
git add .github/workflows/publish-release.yml
git commit -m "feat: add publish-release orchestrator workflow"
```

---

### Task 17: Final verification and documentation

**Step 1: Run the full test suite**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS

**Step 2: Verify all CLI commands are registered**

Run: `cd dev/release-tools && yarn cli --help`
Expected: Shows all 6 commands: `find-last-version`, `bump-version`, `compute-version`, `update-spm-checksum`, `scaffold-notes`, `create-release-branch`

**Step 3: Validate all workflow YAML files**

Run: `python3 -c "import yaml, glob; [yaml.safe_load(open(f)) for f in glob.glob('.github/workflows/release-ios.yml') + glob.glob('.github/workflows/dev-release.yml') + glob.glob('.github/workflows/create-release-branch.yml') + glob.glob('.github/workflows/publish-release.yml')]"`
Expected: No errors

**Step 4: Run repo lint**

Run: `./dev/lint`
Expected: Passes (the new TypeScript files are not covered by the Rust linter)

**Step 5: Final commit if any cleanup was needed**

```bash
git add -A
git commit -m "chore: final cleanup for iOS release process"
```
