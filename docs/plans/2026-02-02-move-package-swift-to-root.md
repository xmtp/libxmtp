# Move Package.swift to Repo Root

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move `sdks/ios/Package.swift` to the repo root so SPM can resolve this package directly from the monorepo, while keeping all iOS dev scripts and CI working.

**Architecture:** Package.swift moves to root with all target paths updated to point into `sdks/ios/`. Dev scripts switch from `cd sdks/ios && swift build` to `cd $ROOT && swift build`. Lint and format scripts stay in `sdks/ios/` since they don't depend on Package.swift location.

**Tech Stack:** Swift Package Manager, bash scripts.

---

### Task 1: Move Package.swift and update paths

**Files:**
- Move: `sdks/ios/Package.swift` → `Package.swift`
- Move: `sdks/ios/.spi.yml` → `.spi.yml`

**Step 1: Create root Package.swift**

Write `Package.swift` at the repo root with the following content. All target `path:` values are updated to point into `sdks/ios/`. The binary target local path changes from `.build/` to `sdks/ios/.build/`. A comment at the top explains why it must stay at root.

```swift
// swift-tools-version: 5.6
// The swift-tools-version declares the minimum version of Swift required to build this package.
//
// NOTE: This file MUST remain at the repository root for Swift Package Manager
// to resolve this package. SPM requires Package.swift at the root of a git
// repository. Do not move it into sdks/ios/ or any subdirectory.

import Foundation
import PackageDescription

let thisPackagePath = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let useLocalBinary = FileManager.default.fileExists(
	atPath: "\(thisPackagePath)/sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
)

let package = Package(
	name: "XMTPiOS",
	platforms: [.iOS(.v14), .macOS(.v11)],
	products: [
		.library(
			name: "XMTPiOS",
			targets: ["XMTPiOS"]
		),
		.library(
			name: "XMTPTestHelpers",
			targets: ["XMTPTestHelpers"]
		),
	],
	dependencies: [
		.package(url: "https://github.com/bufbuild/connect-swift", exact: "1.2.0"),
		.package(url: "https://github.com/apple/swift-docc-plugin.git", from: "1.4.3"),
		.package(url: "https://github.com/krzyzanowskim/CryptoSwift.git", "1.8.4" ..< "2.0.0"),
		.package(url: "https://github.com/SimplyDanny/SwiftLintPlugins", from: "0.62.1"),
	],
	targets: [
		useLocalBinary
			? .binaryTarget(
				name: "LibXMTPSwiftFFI",
				path: "sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
			)
			: .binaryTarget(
				name: "LibXMTPSwiftFFI",
				url: "https://github.com/xmtp/libxmtp/releases/download/ios-0.0.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",
				checksum: "PLACEHOLDER"
			),
		.target(
			name: "XMTPiOS",
			dependencies: [
				.product(name: "Connect", package: "connect-swift"),
				"LibXMTPSwiftFFI",
				.product(name: "CryptoSwift", package: "CryptoSwift"),
			],
			path: "sdks/ios/Sources/XMTPiOS"
		),
		.target(
			name: "XMTPTestHelpers",
			dependencies: ["XMTPiOS"],
			path: "sdks/ios/Sources/XMTPTestHelpers"
		),
		.testTarget(
			name: "XMTPTests",
			dependencies: ["XMTPiOS", "XMTPTestHelpers"],
			path: "sdks/ios/Tests/XMTPTests"
		),
	]
)
```

**Step 2: Move .spi.yml to root**

SPM Index expects `.spi.yml` alongside `Package.swift`.

Copy `sdks/ios/.spi.yml` to `.spi.yml` at the repo root, then delete the original.

**Step 3: Delete old Package.swift**

Remove `sdks/ios/Package.swift`.

**Step 4: Commit**

```bash
git add Package.swift .spi.yml
git rm sdks/ios/Package.swift sdks/ios/.spi.yml
git commit -m "move Package.swift and .spi.yml to repo root for SPM resolution"
```

---

### Task 2: Update root .gitignore for Swift artifacts

**Files:**
- Modify: `.gitignore`

When `swift build` or `swift test` runs from the repo root, SPM creates a `.build` directory and `.swiftpm` directory at the root. These must be ignored.

**Step 1: Add Swift entries to root .gitignore**

Append the following to `.gitignore`:

```
# Swift Package Manager (Package.swift is at root for SPM resolution)
/.build
/.swiftpm
```

**Step 2: Commit**

```bash
git add .gitignore
git commit -m "ignore SPM build artifacts at repo root"
```

---

### Task 3: Update iOS dev scripts

**Files:**
- Modify: `sdks/ios/dev/build`
- Modify: `sdks/ios/dev/test`

The `build` and `test` scripts run `swift build` and `swift test`, which need Package.swift in the working directory. These change to run from `$ROOT` instead of `$IOS_SDK_DIR`. The `lint` and `fmt` scripts use SwiftLint/SwiftFormat which don't depend on Package.swift, so they stay unchanged.

**Step 1: Update `sdks/ios/dev/build`**

```bash
#!/bin/bash
source "$(dirname "$0")/.setup"
ensure_nix_shell "$@"

# Build libxmtp xcframework
cd "${ROOT}/bindings/mobile"
make local

# Build Swift package (Package.swift is at repo root)
cd "${ROOT}"
swift build
```

**Step 2: Update `sdks/ios/dev/test`**

```bash
#!/bin/bash
source "$(dirname "$0")/.setup"
ensure_nix_shell "$@"

# Package.swift is at repo root
cd "${ROOT}"
swift test -q --parallel
```

**Step 3: Verify scripts still work**

Run: `./sdks/ios/dev/build`
Expected: Builds the xcframework and Swift package successfully.

Run: `./sdks/ios/dev/test`
Expected: This requires a running XMTP backend. Verify it at least resolves the package and starts compiling.

**Step 4: Commit**

```bash
git add sdks/ios/dev/build sdks/ios/dev/test
git commit -m "update build and test scripts to use Package.swift at repo root"
```

---

### Task 4: Update GitHub Actions workflows

**Files:**
- Modify: `.github/workflows/test-ios.yaml` (path triggers)
- Modify: `.github/workflows/release-ios.yml` (git add path)

**Step 1: Add `Package.swift` to test-ios.yaml path triggers**

The test workflow triggers on changes to `sdks/ios/**`, but Package.swift is now at root. Add it to the paths.

In `.github/workflows/test-ios.yaml`, update the paths arrays:

```yaml
on:
  push:
    branches: ["main"]
    paths: ["Package.swift", "sdks/ios/**", "bindings/mobile/**", "crates/**"]
  pull_request:
    paths: ["Package.swift", "sdks/ios/**", "bindings/mobile/**", "crates/**"]
```

**Step 2: Update release-ios.yml git add path**

In `.github/workflows/release-ios.yml`, in the "Commit and tag" step, change:

```yaml
git add sdks/ios/Package.swift sdks/ios/XMTP.podspec
```

to:

```yaml
git add Package.swift sdks/ios/XMTP.podspec
```

**Step 3: Commit**

```bash
git add .github/workflows/test-ios.yaml .github/workflows/release-ios.yml
git commit -m "update workflows for Package.swift at repo root"
```

---

### Task 5: Update release-tools SDK config

**Files:**
- Modify: `dev/release-tools/src/lib/sdk-config.ts`
- Modify: `dev/release-tools/tests/spm.test.ts` (if the test fixture path assumptions changed)

**Step 1: Update spmManifestPath in sdk-config.ts**

Change `spmManifestPath` from `"sdks/ios/Package.swift"` to `"Package.swift"`.

**Step 2: Run release-tools tests**

Run: `cd dev/release-tools && yarn test`
Expected: All tests PASS. The spm.test.ts tests use temporary files so they are path-independent.

**Step 3: Commit**

```bash
git add dev/release-tools/src/lib/sdk-config.ts
git commit -m "update SDK config for Package.swift at repo root"
```

---

### Task 6: Update implementation plan and design doc references

**Files:**
- Modify: `docs/plans/2026-02-02-ios-release-process-design.md`
- Modify: `docs/plans/2026-02-02-ios-release-process-implementation.md`

**Step 1: Update design doc**

In the design doc, update references from `sdks/ios/Package.swift` to `Package.swift` (repo root). Update the Package.swift code example to use `sdks/ios/.build/` paths.

**Step 2: Update implementation plan**

In the implementation plan:
- Task 4 (SPM updater): The test fixture is independent, no change needed
- Task 9 (update-spm-checksum command): References `config.spmManifestPath` which is now `Package.swift`
- Task 12 (Package.swift update): Replace entirely - the file is now at root with `sdks/ios/` prefixed paths

**Step 3: Commit**

```bash
git add docs/plans/
git commit -m "update plan docs for Package.swift at repo root"
```

---

### Task 7: Verify everything works end-to-end

**Step 1: Run lint**

Run: `./dev/lint`
Expected: Passes

**Step 2: Run swift package describe from root**

Run: `cd /Users/nickmolnar/code/xmtp/libxmtp && swift package describe`
Expected: Shows package description with correct target paths into `sdks/ios/`

**Step 3: Verify SwiftLint still works**

Run: `./sdks/ios/dev/lint`
Expected: Runs SwiftLint against `sdks/ios/` sources

**Step 4: Verify formatting still works**

Run: `./sdks/ios/dev/fmt --lint`
Expected: Checks formatting of `sdks/ios/` sources

**Step 5: Final commit if any cleanup needed**

```bash
git add -A
git commit -m "chore: final cleanup after Package.swift move"
```
