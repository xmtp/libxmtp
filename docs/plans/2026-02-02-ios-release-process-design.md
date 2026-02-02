# iOS Release Process Design

Implements the release process described in `Release Process (2026).md`, scoped to the iOS SDK.

## Decisions

- **SPM distribution**: Two-commit flow in the monorepo. First commit builds and uploads binaries to a GitHub Release. Second commit updates `Package.swift` with the artifact URL and checksum, then gets the iOS version tag.
- **Package.swift**: Uses conditional logic to detect local `.build/` directory. Local development uses the local binary; external consumers use the remote URL. The release flow only updates the URL and checksum values.
- **Dev releases**: Published to both CocoaPods (prerelease) and SPM (git tag with semver prerelease identifier).
- **Version source of truth**: `XMTP.podspec` (`spec.version` field).
- **Release notes**: Scaffolded by CLI tooling, then refined by the Claude Code GitHub Action.
- **xcframework**: Includes iOS device, iOS simulator, and macOS slices.
- **TypeScript utilities**: Single CLI entrypoint with yargs, also importable as library modules.

## Project Structure

```
dev/release-tools/
  package.json
  tsconfig.json
  src/
    cli.ts                          # yargs entrypoint: release-tools <command>
    commands/
      find-last-version.ts          # Find previous release version for an SDK
      bump-version.ts               # Bump version in SDK manifest
      compute-version.ts            # Compute full version string for a release type
      create-release-branch.ts      # Create and configure a release branch
      scaffold-notes.ts             # Generate release notes template from diff
      update-spm-checksum.ts        # Update Package.swift binary URL and checksum
    lib/
      sdk-config.ts                 # SDK registry: name, manifest path, tag prefix, version parser
      version.ts                    # Semver parsing, comparison, suffix logic
      git.ts                        # Git operations: tags, diffs, branching
      manifest.ts                   # Read/write SDK manifests (podspec, Cargo.toml, etc.)
    types.ts
  tests/
    version.test.ts
    manifest.test.ts
    git.test.ts
    commands/
      find-last-version.test.ts
      bump-version.test.ts
      compute-version.test.ts
      scaffold-notes.test.ts
      update-spm-checksum.test.ts
```

### SDK Config Registry

The SDK config registry maps each SDK to its release metadata. This is the extension point for adding new SDKs later.

For iOS:
- **manifest**: `sdks/ios/XMTP.podspec`
- **spmManifest**: `Package.swift` (repo root, required by SPM)
- **tagPrefix**: `ios-`
- **artifactTagSuffix**: `-libxmtp` (for the intermediate binary release)
- **versionField**: `spec.version` in the podspec

Adding Android, Node, etc. later requires only a new entry in this registry. All commands consume it generically.

## CLI Commands

All commands are subcommands of `release-tools` via yargs. Each exports both a yargs command module and the underlying function for direct import in tests and GitHub Actions.

### `release-tools find-last-version --sdk ios [--pre-release]`

Scans git tags matching the SDK's tag prefix (`ios-*`), parses them as semver, filters out intermediate artifact tags (those ending in `-libxmtp`), and returns the highest version.

- Without `--pre-release`: returns only stable releases (e.g., `4.9.0`)
- With `--pre-release`: includes dev and rc tags (e.g., `4.10.0-rc.1`)
- Returns null/empty with exit code 0 if no previous tags exist

Edge cases to test:
- No matching tags exist
- Mixed stable/prerelease tags with correct ordering
- Artifact tags (`-libxmtp` suffix) are excluded
- Semver ordering across major/minor/patch boundaries

### `release-tools bump-version --sdk ios --type <major|minor|patch>`

Reads the current version from the SDK manifest, applies the semver bump, writes it back. Outputs the new version string to stdout.

### `release-tools compute-version --sdk ios --release-type <dev|rc|final> [--rc-number N]`

Pure computation, no side effects. Takes the base version from the manifest and computes the full version string:
- dev: `4.10.0-dev.abc1234` (short SHA appended)
- rc: `4.10.0-rc.1`
- final: `4.10.0`

### `release-tools update-spm-checksum --sdk ios --url <artifact-url> --checksum <sha256>`

Finds the remote binary target URL and checksum in `Package.swift` and updates them. Fails if the expected pattern is not found. Does not change the conditional logic structure.

### `release-tools scaffold-notes --sdk ios [--since <tag>]`

Finds the last stable release tag (via `find-last-version`), or uses `--since` if provided. Generates a structured markdown template at `docs/release-notes/ios-{version}.md` with:
- Commit messages grouped by conventional commit type
- Files changed summary
- Placeholder sections for breaking changes, new features, fixes

If no previous release tag exists, the template includes a note that this is the first release from the monorepo and does not attempt to traverse the full repo history.

### `release-tools create-release-branch --version <version> --base <ref> --sdk ios --bump <major|minor|patch>`

Orchestrates branch creation:
1. Creates `release/{version}` branch from the base ref
2. Runs `bump-version` for the selected SDK
3. Runs `scaffold-notes` for the selected SDK
4. Commits and pushes

## GitHub Actions Workflows

### Architecture

```
Orchestrator workflows (workflow_dispatch)
  dev-release.yml
  create-release-branch.yml
  publish-release.yml
        |
        | calls (workflow_call)
        v
SDK-specific reusable workflows
  release-ios.yml
  (release-android.yml, etc. in the future)
```

Orchestrators handle SDK selection, dispatch to reusable workflows in parallel, and send Slack notifications after all jobs complete. SDK-specific workflows contain all build/publish logic for that platform.

### `.github/workflows/release-ios.yml` (reusable)

Triggered via `workflow_call`. Inputs:
- `release-type`: `dev` | `rc` | `final`
- `rc-number`: optional, required for rc
- `ref`: git ref to build from

Jobs:

#### 1. `compute-version`

Runs `release-tools compute-version --sdk ios --release-type $TYPE`. Outputs the version string for downstream jobs.

#### 2. `build` (parallel matrix)

Based on the proven pattern from `release-swift-bindings-nix.yml`. Uses a matrix strategy to build each architecture in parallel on `warp-macos-15-arm64-12x` runners:

```yaml
strategy:
  fail-fast: false
  matrix:
    target:
      - aarch64-apple-ios
      - x86_64-apple-ios
      - aarch64-apple-ios-sim
      - x86_64-apple-darwin
      - aarch64-apple-darwin
```

Each job:
- Sets up Nix (`cachix/install-nix-action` + `DeterminateSystems/magic-nix-cache-action` + `Swatinem/rust-cache`)
- Runs `cargo build --release --target $TARGET --manifest-path bindings/mobile/Cargo.toml` under `nix develop`
- Uploads `target/$TARGET/release/libxmtpv3.a` as a workflow artifact

#### 3. `generate-swift-bindings` (parallel with build)

Runs in parallel with the build matrix (no dependency on build artifacts). Sets up Nix and runs `make swift` in `bindings/mobile/` to generate the uniffi Swift bindings (`xmtpv3.swift`, headers, modulemap). Uploads `bindings/mobile/build/swift/` as a workflow artifact.

#### 4. `package` (needs: build, generate-swift-bindings)

Downloads all artifacts from the build and swift jobs. Assembles the release archive:
1. Arranges `Sources/LibXMTP/xmtpv3.swift` from the swift bindings
2. Runs `make framework` to combine the per-architecture `.a` files into `LibXMTPSwiftFFI.xcframework` via lipo + xcodebuild
3. Copies `LICENSE`
4. Creates `LibXMTPSwiftFFI.zip` containing `Sources/`, `LibXMTPSwiftFFI.xcframework/`, and `LICENSE`
5. Computes SHA-256 checksum
6. Creates GitHub Release tagged `ios-{version}-libxmtp`, uploads the zip. If release already exists (re-run), overwrites the asset.

#### 5. `publish` (needs: package)

1. Runs `release-tools update-spm-checksum` with the artifact URL and checksum
2. Updates podspec version (for dev/rc suffixed versions)
3. Commits the Package.swift and podspec changes, tags as `ios-{version}`, pushes
4. Publishes to CocoaPods: `pod trunk push` (prerelease for dev/rc, stable for final). Checks if version already exists on trunk before attempting.
5. For final releases: reads `docs/release-notes/ios-{version}.md` and sets it as the GitHub Release body for the `ios-{version}` tag
6. Outputs the published version string and status

### `.github/workflows/create-release-branch.yml` (orchestrator)

Triggered via `workflow_dispatch`. Inputs:
- Base ref (commit/branch)
- Release version number
- Per-SDK version bump type (currently just iOS: major/minor/patch)

Steps:
1. Run `release-tools create-release-branch` with inputs
2. Trigger Claude Code GitHub Action to open a PR refining the scaffolded release notes

### `.github/workflows/dev-release.yml` (orchestrator)

Triggered via `workflow_dispatch`. Inputs:
- Branch name
- SDK checkboxes (currently just iOS)

For each selected SDK, calls the corresponding reusable workflow with `release-type: dev`. SDKs run in parallel. After all jobs complete, posts to Slack `#notify-dev-releases` (or `#notify-dev-release-failures` on failure).

### `.github/workflows/publish-release.yml` (orchestrator)

Triggered via `workflow_dispatch`. Inputs:
- Release branch
- Release type (RC / Final)
- SDK checkboxes
- RC number (if RC)

For each selected SDK, calls the reusable workflow with the appropriate type. SDKs run in parallel.

For final releases, after all SDK jobs succeed:
- Merge the release branch back to main
- Post to Slack `#notify-sdk-releases`

For failures, post to `#notify-dev-release-failures` with per-SDK status.

## Package.swift Conditional Logic

The Package.swift uses runtime detection to choose between local and remote binaries:

```swift
import Foundation

let thisPackagePath = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let useLocalBinary = FileManager.default.fileExists(
    atPath: "\(thisPackagePath)/sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
)

// In targets array (Package.swift is at repo root):
if useLocalBinary {
    .binaryTarget(
        name: "LibXMTPSwiftFFI",
        path: "sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
    )
} else {
    .binaryTarget(
        name: "LibXMTPSwiftFFI",
        url: "https://github.com/xmtp/libxmtp/releases/download/ios-4.10.0-libxmtp/LibXMTPSwiftFFI.xcframework.zip",
        checksum: "abc123..."
    )
}
```

- Local development in the monorepo: `sdks/ios/.build/` exists after `make local`, uses local binary
- External apps via SPM: `sdks/ios/.build/` doesn't exist, uses remote URL
- Release flow only updates the URL and checksum values in the else branch

## Two-Commit SPM Flow

Inside `release-ios.yml`:

1. **Commit A** (the code being released): Build xcframework, upload to GitHub Release tagged `ios-{version}-libxmtp`. This is the intermediate artifact tag.
2. **Commit B**: Update `Package.swift` url/checksum and podspec version. Tag this commit as `ios-{version}`. This is the tag SPM consumers resolve.

The intermediate `-libxmtp` tag is an implementation detail. Users reference `ios-{version}` only.

## Error Handling

### Re-runnability

All workflows are designed to be re-run safely:
- GitHub Release creation checks if the release/tag already exists. If so, it updates the existing release assets rather than failing.
- The SPM commit/tag step checks if the tag already exists. If the tag points at the expected commit, it skips. If it points elsewhere, it fails with a clear message.
- CocoaPods publish checks if the version is already on trunk before attempting. If already published, it skips with a warning.

### No previous release tag

`find-last-version` returns empty with exit code 0. `scaffold-notes` generates a template noting this is the first monorepo release without traversing full history.

### Branch protection

Workflows use a GitHub App or bot token with permission to push to release branches and create tags. This should be configured as a repository secret.

### Concurrent releases

Git tags are atomic. If two workflows race to create the same GitHub Release, the second fails fast with a clear error. The orchestrator workflows do not attempt to lock.

### Podspec version conflicts

Before `pod trunk push`, the workflow queries CocoaPods to check if the version already exists. If published, it skips with a warning rather than failing the entire workflow.

## CI Infrastructure

### Runners

All iOS build and packaging jobs require macOS runners with Xcode. Use `warp-macos-15-arm64-12x` (12-core ARM64 macOS 15), consistent with the existing `release-swift-bindings-nix.yml` workflow.

### Nix Setup

Every job that compiles Rust or generates bindings uses this pattern:
1. `cachix/install-nix-action@v31` with `access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}`
2. `DeterminateSystems/magic-nix-cache-action@v13`
3. `Swatinem/rust-cache@v2`

### Required Secrets

- `GITHUB_TOKEN`: GitHub Release creation, artifact upload, tag management
- `COCOAPODS_TRUNK_TOKEN`: CocoaPods publish
- `SLACK_WEBHOOK_URL`: Slack notifications
- Bot token or GitHub App credentials for pushing to protected release branches

## Hotfix Support

Hotfix branches (`hotfix/*`) follow the same reusable workflow. A developer:
1. Creates a `hotfix/` branch from an existing release tag
2. Cherry-picks fixes
3. Runs `publish-release.yml` targeting the hotfix branch with release type Final

The `release-ios.yml` reusable workflow doesn't care about branch naming - it only needs the ref and release type.
