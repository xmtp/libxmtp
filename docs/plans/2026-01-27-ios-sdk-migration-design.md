# iOS SDK Migration Design

This document describes the plan to migrate xmtp-ios into the libxmtp monorepo.

## Overview

The xmtp-ios repository will be imported into `sdks/ios/` with full git history preserved. The iOS SDK will use Nix for building the libxmtp dependency and Swift tooling.

## Goals

1. Import xmtp-ios repository into `sdks/ios/` with full history
2. Build the iOS framework using Nix
3. Update scripts and GitHub Actions to work within libxmtp

## Non-Goals

- Publishing releases (out of scope for this migration)
- Dynamic library support changes

---

## 1. Repository Import

Use `git subtree add` without squash to preserve full commit history for blame:

```bash
git subtree add --prefix=sdks/ios https://github.com/xmtp/xmtp-ios.git 01-21-fix_failing_tests
```

### Post-Import Cleanup

**Remove:**
- `sdks/ios/.github/` (workflows move to root)
- `sdks/ios/dev/local/` (docker-compose setup)
- `sdks/ios/dev/up`
- `sdks/ios/dev/start-ngrok-tunnels.sh`
- `sdks/ios/tag_dynamic_library_bindings_release.yml`
- `sdks/ios/triage.yml`
- `sdks/ios/claude_review.yml`

**Keep (even if non-functional):**
- `sdks/ios/XMTP.podspec`
- `sdks/ios/Gemfile`
- `sdks/ios/Gemfile.lock`
- `sdks/ios/tag_and_deploy_to_cocoapods.yml`

**Keep:**
- `sdks/ios/.swiftformat`
- `sdks/ios/.swiftlint.yml`
- `sdks/ios/.swift-version`
- `sdks/ios/dev/fly/` (Fly.io test infrastructure)

### Directory Structure

```
sdks/
└── ios/
    ├── Package.swift
    ├── Sources/
    │   ├── XMTPiOS/
    │   └── XMTPTestHelpers/
    ├── Tests/
    │   └── XMTPTests/
    ├── dev/
    │   ├── build
    │   ├── test
    │   ├── lint
    │   ├── fmt
    │   └── fly/
    │       ├── deploy
    │       ├── cleanup
    │       └── machine-config.json
    ├── .build/                    # Generated (gitignored)
    │   └── LibXMTPSwiftFFI.xcframework
    ├── CLAUDE.md
    ├── .swiftformat
    ├── .swiftlint.yml
    ├── XMTP.podspec
    ├── Gemfile
    └── Gemfile.lock
```

---

## 2. Build System

### Package.swift Modification

Change from remote binaryTarget:

```swift
.binaryTarget(
    name: "LibXMTPSwiftFFI",
    url: "https://github.com/xmtp/libxmtp/releases/...",
    checksum: "..."
)
```

To local path:

```swift
.binaryTarget(
    name: "LibXMTPSwiftFFI",
    path: ".build/LibXMTPSwiftFFI.xcframework"
)
```

### Makefile Changes (`bindings/mobile/Makefile`)

Add output directory variable:

```makefile
IOS_SDK_BUILD_DIR ?= $(WORKSPACE_PATH)/sdks/ios/.build
```

Update `framework` target output path:

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

Add convenience target:

```makefile
# Build everything needed for local iOS SDK development
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework
```

### Gitignore

Add to `sdks/ios/.gitignore`:

```
.build/
```

### Files to Delete

- `.github/workflows/release-swift-bindings.yml`
- `.github/workflows/release-swift-bindings-nix.yml`

---

## 3. Nix Configuration

### Update `nix/ios.nix`

Add Swift tooling to buildInputs:

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

---

## 4. Dev Scripts

All scripts use Nix shell detection pattern:

```bash
if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#ios" --command "$0" "$@"
fi
```

### `sdks/ios/dev/build`

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

### `sdks/ios/dev/test`

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

### `sdks/ios/dev/lint`

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

### `sdks/ios/dev/fmt`

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

---

## 5. GitHub Actions

### `lint-ios.yaml`

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

### `test-ios.yaml`

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

### `cleanup-ios.yaml`

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

### `docs-ios.yaml`

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

---

## 6. CLAUDE.md

Update `sdks/ios/CLAUDE.md`:

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

\`\`\`bash
# From repository root
nix develop .#ios

# Or run scripts directly (they auto-enter Nix shell)
./sdks/ios/dev/build
\`\`\`

## Development Commands

All scripts auto-detect Nix shell and enter it if needed:

\`\`\`bash
./sdks/ios/dev/build    # Build libxmtp xcframework + Swift package
./sdks/ios/dev/test     # Run Swift tests
./sdks/ios/dev/lint     # Run SwiftLint
./sdks/ios/dev/fmt      # Format code with SwiftFormat
./sdks/ios/dev/fmt --lint  # Check formatting without changes
\`\`\`

## Building the xcframework

The Swift package depends on `LibXMTPSwiftFFI.xcframework` which is built from the Rust code in `bindings/mobile/`. Run `./sdks/ios/dev/build` to rebuild it when Rust code changes.

The xcframework is output to `.build/LibXMTPSwiftFFI.xcframework`.

## Testing

Tests require a running XMTP backend. For CI, tests use ephemeral Fly.io infrastructure. For local testing:

\`\`\`bash
# Start local backend (from repo root)
./dev/docker/up

# Run tests
./sdks/ios/dev/test
\`\`\`

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

---

## 7. Complete File Change Summary

### Files to Create

| File | Description |
|------|-------------|
| `.github/workflows/lint-ios.yaml` | SwiftLint and SwiftFormat CI |
| `.github/workflows/test-ios.yaml` | iOS integration tests with Fly.io |
| `.github/workflows/cleanup-ios.yaml` | Hourly cleanup of stale test apps |
| `.github/workflows/docs-ios.yaml` | Documentation generation |
| `sdks/ios/` | Imported via git subtree |
| `sdks/ios/dev/build` | New build script |
| `sdks/ios/dev/test` | New test script |

### Files to Modify

| File | Change |
|------|--------|
| `nix/ios.nix` | Add swiftformat, swiftlint to buildInputs |
| `bindings/mobile/Makefile` | Update framework output path, add `local` target |
| `sdks/ios/Package.swift` | Change binaryTarget to local path |
| `sdks/ios/dev/lint` | Update for Nix shell detection |
| `sdks/ios/dev/fmt` | Update for Nix shell detection |
| `sdks/ios/.gitignore` | Add `.build/` |
| `sdks/ios/CLAUDE.md` | Complete rewrite |

### Files to Delete

| File | Reason |
|------|--------|
| `.github/workflows/release-swift-bindings.yml` | Releases out of scope |
| `.github/workflows/release-swift-bindings-nix.yml` | Releases out of scope |
| `sdks/ios/.github/` | Workflows moved to root |
| `sdks/ios/dev/local/` | Use libxmtp's docker-compose |
| `sdks/ios/dev/up` | Use libxmtp's dev/up |
| `sdks/ios/dev/start-ngrok-tunnels.sh` | Replaced by Fly.io |
| `sdks/ios/tag_dynamic_library_bindings_release.yml` | Releases out of scope |
| `sdks/ios/triage.yml` | Not needed |
| `sdks/ios/claude_review.yml` | Not needed |

### Files to Keep (non-functional)

| File | Reason |
|------|--------|
| `sdks/ios/XMTP.podspec` | Future releases |
| `sdks/ios/Gemfile` | Future releases |
| `sdks/ios/Gemfile.lock` | Future releases |
| `sdks/ios/tag_and_deploy_to_cocoapods.yml` | Future releases |

---

## Implementation Order

1. Import xmtp-ios via git subtree
2. Delete/cleanup files as specified
3. Update `nix/ios.nix` with Swift tooling
4. Update `bindings/mobile/Makefile`
5. Update `sdks/ios/Package.swift`
6. Create/update dev scripts
7. Create GitHub Actions workflows
8. Update `sdks/ios/CLAUDE.md`
9. Update `sdks/ios/.gitignore`
10. Validate build and tests work locally
