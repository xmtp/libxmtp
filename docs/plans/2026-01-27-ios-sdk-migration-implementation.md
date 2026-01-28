# iOS SDK Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Import xmtp-ios into libxmtp monorepo with Nix-based build system and working CI.

**Architecture:** Git subtree import preserving history, Nix shell for Rust cross-compilation and Swift tooling, Fly.io for ephemeral test infrastructure in CI.

**Tech Stack:** Rust, Swift, Nix, GitHub Actions, Fly.io, SwiftFormat, SwiftLint

---

## Task 1: Import xmtp-ios Repository

**Files:**
- Create: `sdks/ios/` (via git subtree)

**Step 1: Import repository with full history**

```bash
git subtree add --prefix=sdks/ios https://github.com/xmtp/xmtp-ios.git 01-21-fix_failing_tests
```

Expected: Merge commit created, `sdks/ios/` populated with xmtp-ios contents

**Step 2: Verify import**

```bash
ls sdks/ios/Package.swift sdks/ios/Sources/XMTPiOS/
git log --oneline sdks/ios/Sources/XMTPiOS/Client.swift | head -5
```

Expected: Files exist, git log shows original commit history

**Step 3: Commit checkpoint**

No additional commit needed - subtree add creates merge commit automatically.

---

## Task 2: Clean Up Imported Files

**Files:**
- Delete: `sdks/ios/.github/`
- Delete: `sdks/ios/dev/local/`
- Delete: `sdks/ios/dev/up`
- Delete: `sdks/ios/dev/start-ngrok-tunnels.sh`

**Step 1: Remove files that are no longer needed**

```bash
rm -rf sdks/ios/.github/
rm -rf sdks/ios/dev/local/
rm -f sdks/ios/dev/up
rm -f sdks/ios/dev/start-ngrok-tunnels.sh
rm -f sdks/ios/script/local
rm -f sdks/ios/script/gen-proto
rm -f sdks/ios/script/docs
```

**Step 2: Remove build artifacts that may have been committed**

```bash
rm -f sdks/ios/ReactionV2Codec.d sdks/ios/ReactionV2Codec.o sdks/ios/ReactionV2Codec.swiftdeps
rm -rf sdks/ios/.swiftpm/
```

**Step 3: Verify kept files exist**

```bash
ls sdks/ios/XMTP.podspec sdks/ios/Gemfile sdks/ios/Gemfile.lock
ls sdks/ios/.swiftformat sdks/ios/.swiftlint.yml
ls sdks/ios/dev/fly/deploy sdks/ios/dev/fly/cleanup sdks/ios/dev/fly/machine-config.json
```

Expected: All files exist

**Step 4: Commit cleanup**

```bash
git add -A
git commit -m "Clean up imported xmtp-ios files

Remove:
- .github/ (workflows move to root)
- dev/local/ (use libxmtp docker-compose)
- dev/up (use libxmtp dev/up)
- ngrok scripts (replaced by Fly.io)
- Build artifacts

Keep:
- Podspec and Gemfiles for future releases
- Fly.io test infrastructure
- SwiftFormat/SwiftLint configs

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Update Nix iOS Shell

**Files:**
- Modify: `nix/ios.nix`

**Step 1: Read current ios.nix**

```bash
cat nix/ios.nix
```

**Step 2: Add Swift tooling to buildInputs**

Edit `nix/ios.nix` to add swiftformat and swiftlint:

```nix
buildInputs =
  [
    rust-ios-toolchain

    # native libs
    zstd
    openssl
    sqlite
    xcbuild
    # Swift tooling
    pkgs.swiftformat
    pkgs.swiftlint
  ]
  ++ lib.optionals isDarwin [
    darwin.cctools
  ];
```

**Step 3: Verify Nix shell has Swift tools**

```bash
nix develop .#ios --command which swiftformat
nix develop .#ios --command which swiftlint
nix develop .#ios --command swiftformat --version
nix develop .#ios --command swiftlint version
```

Expected: Both tools found with version output

**Step 4: Commit**

```bash
git add nix/ios.nix
git commit -m "Add Swift tooling to Nix iOS shell

Include swiftformat and swiftlint in the iOS development shell
for consistent formatting and linting across environments.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Update Makefile

**Files:**
- Modify: `bindings/mobile/Makefile`

**Step 1: Read current Makefile framework target**

```bash
grep -A 10 "^framework:" bindings/mobile/Makefile
```

**Step 2: Add IOS_SDK_BUILD_DIR variable and update framework target**

Add near top of Makefile after existing variables:

```makefile
IOS_SDK_BUILD_DIR ?= $(WORKSPACE_PATH)/sdks/ios/.build
```

Update framework target to output to new location:

```makefile
framework: lipo
	mkdir -p $(IOS_SDK_BUILD_DIR)
	rm -rf $(IOS_SDK_BUILD_DIR)/LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library build/aarch64-apple-ios/$(LIB) \
		-headers build/swift/static/include/libxmtp/ \
		-library build/lipo_ios_sim/$(LIB) \
		-headers build/swift/static/include/libxmtp/ \
		-library build/lipo_macos/$(LIB) \
		-headers build/swift/static/include/libxmtp/ \
		-output $(IOS_SDK_BUILD_DIR)/LibXMTPSwiftFFI.xcframework
```

**Step 3: Add local convenience target**

Add at end of Makefile:

```makefile
# Build everything needed for local iOS SDK development
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework
```

**Step 4: Update .PHONY**

Add `local` to the .PHONY line.

**Step 5: Commit**

```bash
git add bindings/mobile/Makefile
git commit -m "Update Makefile for iOS SDK integration

- Add IOS_SDK_BUILD_DIR variable for xcframework output
- Update framework target to output to sdks/ios/.build/
- Add 'local' convenience target for full iOS build

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Update Package.swift

**Files:**
- Modify: `sdks/ios/Package.swift`

**Step 1: Read current Package.swift**

```bash
cat sdks/ios/Package.swift
```

**Step 2: Change binaryTarget from URL to local path**

Replace the binaryTarget block:

```swift
.binaryTarget(
    name: "LibXMTPSwiftFFI",
    path: ".build/LibXMTPSwiftFFI.xcframework"
),
```

**Step 3: Commit**

```bash
git add sdks/ios/Package.swift
git commit -m "Update Package.swift for local xcframework path

Change LibXMTPSwiftFFI from remote URL to local path at
.build/LibXMTPSwiftFFI.xcframework for monorepo development.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Update .gitignore

**Files:**
- Modify: `sdks/ios/.gitignore`

**Step 1: Read current .gitignore**

```bash
cat sdks/ios/.gitignore
```

**Step 2: Add .build/ directory**

Add to sdks/ios/.gitignore:

```
# Built xcframework
.build/
```

**Step 3: Commit**

```bash
git add sdks/ios/.gitignore
git commit -m "Add .build/ to iOS SDK gitignore

Ignore generated xcframework directory.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 7: Create Dev Scripts

**Files:**
- Create: `sdks/ios/dev/build`
- Create: `sdks/ios/dev/test`
- Modify: `sdks/ios/dev/lint`
- Modify: `sdks/ios/dev/fmt`

**Step 1: Create build script**

Create `sdks/ios/dev/build`:

```bash
#!/bin/bash
set -eou pipefail
ROOT="$(git rev-parse --show-toplevel)"

if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#ios" --command "$0" "$@"
fi

# Build libxmtp xcframework
cd "${ROOT}/bindings/mobile"
make local

# Build Swift package
cd "${ROOT}/sdks/ios"
swift build
```

```bash
chmod +x sdks/ios/dev/build
```

**Step 2: Create test script**

Create `sdks/ios/dev/test`:

```bash
#!/bin/bash
set -eou pipefail
ROOT="$(git rev-parse --show-toplevel)"

if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#ios" --command "$0" "$@"
fi

cd "${ROOT}/sdks/ios"
swift test -q --parallel --num-workers=2
```

```bash
chmod +x sdks/ios/dev/test
```

**Step 3: Update lint script**

Replace `sdks/ios/dev/lint`:

```bash
#!/bin/bash
set -eou pipefail
ROOT="$(git rev-parse --show-toplevel)"

if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#ios" --command "$0" "$@"
fi

cd "${ROOT}/sdks/ios"
swiftlint lint .
swiftlint lint --config Tests/.swiftlint.yml ./Tests
```

**Step 4: Update fmt script**

Replace `sdks/ios/dev/fmt`:

```bash
#!/bin/bash
set -eou pipefail
ROOT="$(git rev-parse --show-toplevel)"

if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#ios" --command "$0" "$@"
fi

cd "${ROOT}/sdks/ios"
swiftformat . "$@"
```

**Step 5: Commit**

```bash
git add sdks/ios/dev/
git commit -m "Update iOS dev scripts for Nix shell integration

- build: Builds xcframework and Swift package
- test: Runs Swift tests
- lint: Runs SwiftLint with Nix shell detection
- fmt: Runs SwiftFormat with Nix shell detection

All scripts auto-enter Nix shell if not already in one.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 8: Create GitHub Actions - Lint

**Files:**
- Create: `.github/workflows/lint-ios.yaml`

**Step 1: Create lint workflow**

Create `.github/workflows/lint-ios.yaml`:

```yaml
name: Lint iOS

on:
  push:
    branches: ["main"]
    paths: ["sdks/ios/**"]
  pull_request:
    paths: ["sdks/ios/**"]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  swiftlint:
    name: SwiftLint
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - name: Run SwiftLint
        run: nix develop .#ios --command ./sdks/ios/dev/lint

  swiftformat:
    name: SwiftFormat
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - name: Check SwiftFormat
        run: nix develop .#ios --command ./sdks/ios/dev/fmt --lint
```

**Step 2: Commit**

```bash
git add .github/workflows/lint-ios.yaml
git commit -m "Add iOS lint GitHub Action

Runs SwiftLint and SwiftFormat checks on iOS SDK changes.
Uses Nix for consistent tooling.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 9: Create GitHub Actions - Test

**Files:**
- Create: `.github/workflows/test-ios.yaml`

**Step 1: Create test workflow**

Create `.github/workflows/test-ios.yaml`:

```yaml
name: iOS Tests

on:
  push:
    branches: ["main"]
    paths: ["sdks/ios/**", "bindings/mobile/**", "crates/**"]
  pull_request:
    paths: ["sdks/ios/**", "bindings/mobile/**", "crates/**"]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  deploy-backend:
    runs-on: ubuntu-latest
    timeout-minutes: 10
    outputs:
      node_url: ${{ steps.deploy.outputs.node_url }}
      history_url: ${{ steps.deploy.outputs.history_url }}
      app_name: ${{ steps.deploy.outputs.app_name }}
    steps:
      - uses: actions/checkout@v4
      - uses: superfly/flyctl-actions/setup-flyctl@master
      - name: Deploy test infrastructure
        id: deploy
        env:
          FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
        run: |
          APP_NAME="libxmtp-ios-test-${{ github.run_id }}-${{ github.run_attempt }}"
          ./sdks/ios/dev/fly/deploy "$APP_NAME"
          echo "app_name=$APP_NAME" >> "$GITHUB_OUTPUT"
          echo "node_url=https://${APP_NAME}.fly.dev" >> "$GITHUB_OUTPUT"
          echo "history_url=https://${APP_NAME}.fly.dev:5558" >> "$GITHUB_OUTPUT"
      - name: Wait for services
        env:
          FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
        run: |
          NODE_HOST="${{ steps.deploy.outputs.app_name }}.fly.dev"
          for i in $(seq 1 60); do
            if echo | openssl s_client -connect "${NODE_HOST}:443" -servername "${NODE_HOST}" 2>/dev/null | grep -q "CONNECTED"; then
              echo "Services ready"
              exit 0
            fi
            echo "Waiting... ($i/60)"
            sleep 5
          done
          exit 1

  tests:
    runs-on: warp-macos-15-arm64-6x
    needs: deploy-backend
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - uses: maxim-lobanov/setup-xcode@v1
        with:
          xcode-version: "16.0"
      - name: Build
        run: nix develop .#ios --command ./sdks/ios/dev/build
      - name: Run tests
        env:
          XMTP_NODE_ADDRESS: ${{ needs.deploy-backend.outputs.node_url }}
          XMTP_HISTORY_SERVER_ADDRESS: ${{ needs.deploy-backend.outputs.history_url }}
        run: nix develop .#ios --command ./sdks/ios/dev/test

  cleanup:
    runs-on: ubuntu-latest
    needs: [deploy-backend, tests]
    if: always()
    steps:
      - uses: superfly/flyctl-actions/setup-flyctl@master
      - name: Destroy app
        env:
          FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
        run: |
          APP_NAME="${{ needs.deploy-backend.outputs.app_name }}"
          [ -n "$APP_NAME" ] && flyctl apps destroy "$APP_NAME" --yes || true
```

**Step 2: Commit**

```bash
git add .github/workflows/test-ios.yaml
git commit -m "Add iOS test GitHub Action

Runs integration tests with ephemeral Fly.io backend.
Triggers on iOS SDK, bindings, and crate changes.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 10: Create GitHub Actions - Cleanup and Docs

**Files:**
- Create: `.github/workflows/cleanup-ios.yaml`
- Create: `.github/workflows/docs-ios.yaml`

**Step 1: Create cleanup workflow**

Create `.github/workflows/cleanup-ios.yaml`:

```yaml
name: Cleanup iOS Test Apps

on:
  schedule:
    - cron: "0 * * * *"
  workflow_dispatch:
    inputs:
      dry_run:
        description: "Dry run"
        default: false
        type: boolean

jobs:
  cleanup:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: superfly/flyctl-actions/setup-flyctl@master
      - name: Clean up stale apps
        env:
          FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
        run: |
          ARGS=""
          [ "${{ inputs.dry_run }}" = "true" ] && ARGS="--dry-run"
          ./sdks/ios/dev/fly/cleanup $ARGS --max-age 2
```

**Step 2: Create docs workflow**

Create `.github/workflows/docs-ios.yaml`:

```yaml
name: iOS Docs

on:
  push:
    branches: ["main"]
    paths: ["sdks/ios/**"]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: "ios-pages"
  cancel-in-progress: false

jobs:
  deploy:
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    runs-on: macos-13
    steps:
      - uses: actions/checkout@v4
      - name: Set up Ruby
        uses: ruby/setup-ruby@v1
        with:
          ruby-version: "3.0"
      - name: Cache Ruby gems
        uses: actions/cache@v4
        with:
          path: vendor/bundle
          key: ${{ runner.os }}-gems-${{ hashFiles('**/Gemfile.lock') }}
      - name: Install Jazzy
        working-directory: sdks/ios
        run: |
          bundle config path vendor/bundle
          bundle add jazzy
      - name: Generate documentation
        working-directory: sdks/ios
        run: bundle exec jazzy --output ./docs --theme=fullwidth --module=XMTPiOS
      - name: Setup Pages
        uses: actions/configure-pages@v5
      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: "sdks/ios/docs"
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

**Step 3: Commit**

```bash
git add .github/workflows/cleanup-ios.yaml .github/workflows/docs-ios.yaml
git commit -m "Add iOS cleanup and docs GitHub Actions

- cleanup-ios: Hourly cleanup of stale Fly.io test apps
- docs-ios: Generate and deploy iOS SDK documentation

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 11: Delete Release Workflows

**Files:**
- Delete: `.github/workflows/release-swift-bindings.yml`
- Delete: `.github/workflows/release-swift-bindings-nix.yml`

**Step 1: Delete files**

```bash
rm -f .github/workflows/release-swift-bindings.yml
rm -f .github/workflows/release-swift-bindings-nix.yml
```

**Step 2: Commit**

```bash
git add -A
git commit -m "Remove Swift release workflows

Releases are out of scope for the initial iOS SDK migration.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 12: Update CLAUDE.md

**Files:**
- Modify: `sdks/ios/CLAUDE.md`

**Step 1: Replace CLAUDE.md content**

Replace entire contents of `sdks/ios/CLAUDE.md` with:

```markdown
# XMTP iOS SDK - Claude Assistant Context

This SDK provides Swift bindings for the XMTP messaging protocol, built on top of libxmtp.

## Project Structure

- `Sources/XMTPiOS/` - Main SDK source code
- `Sources/XMTPTestHelpers/` - Test utilities
- `Tests/XMTPTests/` - Test suite
- `dev/` - Development scripts
- `.build/` - Generated artifacts (gitignored)

## Development Setup

This SDK lives within the libxmtp monorepo. Use the Nix development environment:

```bash
# From repository root
nix develop .#ios

# Or run scripts directly (they auto-enter Nix shell)
./sdks/ios/dev/build
```

## Development Commands

All scripts auto-detect Nix shell and enter it if needed:

```bash
./sdks/ios/dev/build    # Build libxmtp xcframework + Swift package
./sdks/ios/dev/test     # Run Swift tests
./sdks/ios/dev/lint     # Run SwiftLint
./sdks/ios/dev/fmt      # Format code with SwiftFormat
./sdks/ios/dev/fmt --lint  # Check formatting without changes
```

## Building the xcframework

The Swift package depends on `LibXMTPSwiftFFI.xcframework` which is built from the Rust code in `bindings/mobile/`. Run `./sdks/ios/dev/build` to rebuild it when Rust code changes.

The xcframework is output to `.build/LibXMTPSwiftFFI.xcframework`.

## Testing

Tests require a running XMTP backend. For CI, tests use ephemeral Fly.io infrastructure. For local testing:

```bash
# Start local backend (from repo root)
./dev/docker/up

# Run tests
./sdks/ios/dev/test
```

Environment variables for custom backend:
- `XMTP_NODE_ADDRESS` - Node gRPC URL
- `XMTP_HISTORY_SERVER_ADDRESS` - History server URL

## Code Style

- **Formatting**: SwiftFormat (nicklockwood) - config in `.swiftformat`
- **Linting**: SwiftLint - config in `.swiftlint.yml` and `Tests/.swiftlint.yml`

## Key Dependencies

- `LibXMTPSwiftFFI` - FFI bindings from libxmtp (local path)
- `Connect` - gRPC client
- `CryptoSwift` - Cryptographic utilities
```

**Step 2: Commit**

```bash
git add sdks/ios/CLAUDE.md
git commit -m "Update iOS SDK CLAUDE.md for monorepo

Document development workflow, commands, and testing
for the iOS SDK within the libxmtp monorepo.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 13: Update Fly.io Scripts

**Files:**
- Modify: `sdks/ios/dev/fly/cleanup`

**Step 1: Update APP_PREFIX in cleanup script**

Change `APP_PREFIX="xmtp-ios-test"` to `APP_PREFIX="libxmtp-ios-test"` in `sdks/ios/dev/fly/cleanup`.

**Step 2: Commit**

```bash
git add sdks/ios/dev/fly/cleanup
git commit -m "Update Fly.io cleanup script for libxmtp prefix

Match the app naming used in test-ios.yaml.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 14: Local Validation - Build

**Goal:** Verify the build script works end-to-end locally.

**Step 1: Clean any existing build artifacts**

```bash
rm -rf sdks/ios/.build/
rm -rf bindings/mobile/build/
```

**Step 2: Run build script**

```bash
./sdks/ios/dev/build
```

Expected:
- Rust compilation succeeds for all iOS targets
- xcframework created at `sdks/ios/.build/LibXMTPSwiftFFI.xcframework`
- Swift package builds successfully

**Step 3: Verify xcframework exists**

```bash
ls -la sdks/ios/.build/LibXMTPSwiftFFI.xcframework/
```

Expected: Directory with ios-arm64, ios-arm64_x86_64-simulator, macos-arm64_x86_64 subdirectories

**Step 4: Verify Swift build artifacts**

```bash
ls sdks/ios/.build/debug/ 2>/dev/null || ls sdks/ios/.swiftpm/
```

Expected: Swift build artifacts present

---

## Task 15: Local Validation - Format

**Goal:** Verify the format script works locally.

**Step 1: Run format check (lint mode)**

```bash
./sdks/ios/dev/fmt --lint
```

Expected: Either passes (exit 0) or shows formatting differences

**Step 2: Run format (if needed)**

```bash
./sdks/ios/dev/fmt
```

Expected: Files formatted, exit 0

**Step 3: Verify no changes needed**

```bash
./sdks/ios/dev/fmt --lint
```

Expected: Exit 0, no output about changes needed

---

## Task 16: Local Validation - Lint

**Goal:** Verify the lint script works locally.

**Step 1: Run lint**

```bash
./sdks/ios/dev/lint
```

Expected: SwiftLint runs on Sources and Tests directories

Note: Some warnings may be present - this is expected. The goal is that the script runs successfully.

---

## Task 17: Local Validation - Test (with local backend)

**Goal:** Verify tests can run against local docker backend.

**Step 1: Start local backend (from repo root)**

```bash
./dev/docker/up
```

Expected: Docker services start (node, validation, db, etc.)

**Step 2: Wait for services**

```bash
timeout 60 bash -c 'until nc -z localhost 5556; do sleep 1; done' && echo "Ready"
```

Expected: "Ready" printed

**Step 3: Run tests**

```bash
XMTP_NODE_ADDRESS=http://localhost:5556 ./sdks/ios/dev/test
```

Expected: Tests run (some may fail if backend isn't fully compatible - document any issues)

**Step 4: Stop backend**

```bash
./dev/docker/down
```

---

## Task 18: Validation Summary Document

**Files:**
- Create: `sdks/ios/VALIDATION.md` (temporary, for PR description)

**Step 1: Document validation results**

Create a summary of what was validated:

```markdown
# iOS SDK Migration Validation

## Local Validation Results

### Build (`./sdks/ios/dev/build`)
- [ ] Rust cross-compilation succeeds for all targets
- [ ] xcframework created at correct location
- [ ] Swift package builds successfully

### Format (`./sdks/ios/dev/fmt`)
- [ ] Format check runs successfully
- [ ] SwiftFormat version: X.X.X

### Lint (`./sdks/ios/dev/lint`)
- [ ] SwiftLint runs on Sources/
- [ ] SwiftLint runs on Tests/
- [ ] SwiftLint version: X.X.X

### Test (`./sdks/ios/dev/test`)
- [ ] Tests run against local docker backend
- [ ] Test results: X passed, Y failed (if any)

## CI Validation

### lint-ios.yaml
- [ ] SwiftLint job configuration correct
- [ ] SwiftFormat job configuration correct
- [ ] Path filters set correctly

### test-ios.yaml
- [ ] Fly.io deployment step configured
- [ ] Build step uses Nix shell
- [ ] Test step receives backend URLs
- [ ] Cleanup step runs on failure

### cleanup-ios.yaml
- [ ] Cron schedule correct
- [ ] Uses updated app prefix

### docs-ios.yaml
- [ ] Path filters set correctly
- [ ] Jazzy generation configured

## Notes

[Document any issues or deviations from plan]
```

**Step 2: Commit validation document**

```bash
git add sdks/ios/VALIDATION.md
git commit -m "Add validation checklist for iOS SDK migration

Document local and CI validation status.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 19: Final Review and Cleanup

**Step 1: Run full validation sequence**

```bash
# Format
./sdks/ios/dev/fmt --lint

# Lint
./sdks/ios/dev/lint

# Build (full)
rm -rf sdks/ios/.build/
./sdks/ios/dev/build
```

**Step 2: Verify all files are committed**

```bash
git status
```

Expected: Clean working tree

**Step 3: Review commit history**

```bash
git log --oneline main..HEAD
```

Expected: Clear sequence of commits for migration

**Step 4: Update VALIDATION.md with final results**

Fill in all checkboxes with actual validation results.

```bash
git add sdks/ios/VALIDATION.md
git commit -m "Update validation results

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 20: Merge Preparation

**Step 1: Ensure branch is up to date with main**

```bash
git fetch origin main
git rebase origin/main
```

**Step 2: Final verification after rebase**

```bash
./sdks/ios/dev/build
./sdks/ios/dev/fmt --lint
./sdks/ios/dev/lint
```

**Step 3: Push branch**

```bash
git push -u origin ios-sdk-migration
```

**Step 4: Create PR with validation summary**

Include VALIDATION.md contents in PR description.

---

## Validation Checklist Summary

| Component | Local Test | CI Config |
|-----------|------------|-----------|
| Build | `./sdks/ios/dev/build` succeeds | test-ios.yaml build step |
| Format | `./sdks/ios/dev/fmt --lint` passes | lint-ios.yaml swiftformat job |
| Lint | `./sdks/ios/dev/lint` runs | lint-ios.yaml swiftlint job |
| Test | `./sdks/ios/dev/test` with docker | test-ios.yaml with Fly.io |

All local validations must pass before considering the migration complete.
