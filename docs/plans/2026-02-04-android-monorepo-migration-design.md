# Android SDK Monorepo Migration Design

This document describes the design for migrating xmtp-android into the libxmtp monorepo, adapting it to work with Nix-based builds and the existing monorepo structure.

## Goals

- Local development environment configured via Nix and direnv
- Android bindings built as a Nix package derivation with Cachix caching
- CI workflows migrated to monorepo root
- Documentation updated for new structure

## Directory Structure

Target structure for `sdks/android/` after migration:

```
sdks/android/
├── .build/                    # Generated artifacts (gitignored)
│   └── bindings/
│       ├── java/
│       │   └── xmtpv3.kt
│       ├── jniLibs/
│       │   ├── arm64-v8a/libuniffi_xmtpv3.so
│       │   ├── armeabi-v7a/libuniffi_xmtpv3.so
│       │   ├── x86/libuniffi_xmtpv3.so
│       │   └── x86_64/libuniffi_xmtpv3.so
│       └── libxmtp-version.txt
├── .direnv/                   # direnv cache (gitignored)
├── .envrc                     # `use flake ../../#android`
├── .gitignore                 # Updated for .build/, .direnv/
├── dev/
│   ├── .setup                 # Shared setup script
│   ├── bindings               # Build bindings via Nix
│   ├── build                  # Full SDK build
│   └── fmt                    # Existing formatter
├── library/
│   └── build.gradle           # Updated sourceSets
├── example/
├── gradle/
├── build.gradle
├── settings.gradle
├── CLAUDE.md
└── README.md
```

## Design Decisions

### Bindings Build Approach

Full Nix derivation (matching iOS pattern):
- `nix/package/android.nix` builds all 4 Android targets + Kotlin bindings
- Cached via Cachix for fast CI and developer builds
- `sdks/android/dev/bindings` script fetches from Nix

### Generated Files Location

Dedicated `.build/` directory:
- Generated files go to `.build/bindings/`
- Gradle configured with additional sourceSets
- Clean separation from source files

### Nix Shell Entry

Simple flake reference:
- `.envrc` with `use flake ../../#android --accept-flake-config`
- Auto-enters devShell when entering `sdks/android/`
- Single shell includes all tooling (JDK, Kotlin, Android SDK, Rust)

### CI Test Infrastructure

Docker-based with `dev/up`:
- Consistent with local development
- Simpler than Fly.io ephemeral infrastructure
- Proven approach from existing Android CI

## Nix Package Derivation

New `nix/package/android.nix` will build:

1. Per-target `.so` libraries for 4 Android architectures:
   - `aarch64-linux-android` → `arm64-v8a`
   - `armv7-linux-androideabi` → `armeabi-v7a`
   - `x86_64-linux-android` → `x86_64`
   - `i686-linux-android` → `x86`

2. Kotlin bindings via uniffi-bindgen:
   - `xmtpv3.kt` with required sed replacements
   - `libxmtp-version.txt`

Structure:
```nix
{
  targets = {
    "aarch64-linux-android" = <drv>;
    "armv7-linux-androideabi" = <drv>;
    "x86_64-linux-android" = <drv>;
    "i686-linux-android" = <drv>;
  };
  kotlinBindings = <drv>;
  aggregate = <drv>;  # nix build .#android-libs
}
```

Cache strategy:
- Crane two-phase build (buildDepsOnly → buildPackage)
- Each target cached independently
- Kotlin bindings cached separately

## Dev Scripts

### `dev/.setup`
```bash
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SDK_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

ensure_nix_shell() {
  if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix develop "${ROOT}#android" --command "$0" "$@"
  fi
}
```

### `dev/bindings`
- Runs `nix build "${ROOT}#android-libs"`
- Copies outputs to `.build/bindings/` with correct directory structure
- Maps Rust targets to Android ABI names

### `dev/build`
- Ensures bindings are built via `dev/bindings`
- Runs `./gradlew build`

## Gradle Changes

`library/build.gradle` sourceSets:
```gradle
android {
    sourceSets {
        main {
            java.srcDirs += ["${rootProject.projectDir}/.build/bindings/java"]
            jniLibs.srcDirs += ["${rootProject.projectDir}/.build/bindings/jniLibs"]
        }
    }
}
```

Project name remains `xmtp-android`.

## CI Workflows

### `lint-android.yml`
```yaml
name: Lint Android

on:
  push:
    branches: ["main"]
    paths: ["sdks/android/**"]
  pull_request:
    paths: ["sdks/android/**"]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
      - name: Run Spotless Check
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android spotlessCheck
      - name: Run Android Lint
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android :library:lintDebug
```

### `test-android.yml`
```yaml
name: Android Tests

on:
  push:
    branches: ["main"]
    paths: ["sdks/android/**", "bindings/mobile/**", "crates/**"]
  pull_request:
    paths: ["sdks/android/**", "bindings/mobile/**", "crates/**"]

jobs:
  unit-tests:
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
      - name: Build bindings
        run: nix develop .#android --command ./sdks/android/dev/bindings
      - name: Start backend
        run: ./dev/up
      - name: Run unit tests
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android library:testDebug

  integration-tests:
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
      - name: Enable KVM
        run: |
          echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules
          sudo udevadm control --reload-rules
          sudo udevadm trigger --name-match=kvm
      - name: Build bindings
        run: nix develop .#android --command ./sdks/android/dev/bindings
      - name: Start backend
        run: ./dev/up
      - name: Run integration tests
        uses: reactivecircus/android-emulator-runner@v2
        with:
          api-level: 34
          arch: x86_64
          script: nix develop .#android --command ./sdks/android/gradlew -p sdks/android connectedCheck
```

All Java/Kotlin tooling comes from the Nix devShell (no separate setup-java step).

## Files to Delete

- `sdks/android/script/` - Legacy scripts for old repo
- `sdks/android/libxmtp/` - Empty directory
- `sdks/android/.github/workflows/claude-review.yml`
- `sdks/android/.github/workflows/triage.yml`
- `sdks/android/.github/workflows/docs.yml`
- `dev/release-kotlin` - Replaced by `sdks/android/dev/bindings`

## Files to Keep (for later)

- `sdks/android/.github/workflows/dev-release.yml`
- `sdks/android/.github/workflows/release.yml`

## Documentation Updates

### Root README.md
Add Android SDK reference in SDKs section.

### Root CLAUDE.md
Add Android commands and devShell reference.

### sdks/android/README.md
- Remove standalone repo references
- Update for Nix/direnv setup
- Update build commands for dev/ scripts

### sdks/android/CLAUDE.md
- Rewrite for monorepo context
- Document dev/ scripts and Nix devShell
- Remove references to deleted directories

## Implementation Plan

### PR 1: Local Build Setup
Branch: `gt create -am "feat(android): local build setup with Nix bindings"`

1. Create `nix/package/android.nix`
2. Add `packages.android-libs` to `flake.nix`
3. Create `sdks/android/.envrc`
4. Create `sdks/android/dev/.setup`, `dev/bindings`, `dev/build`
5. Update `sdks/android/library/build.gradle` sourceSets
6. Update `sdks/android/.gitignore`
7. Delete `sdks/android/script/`, `sdks/android/libxmtp/`
8. Delete `dev/release-kotlin`

### PR 2: CI Workflows
Branch: `gt create -am "feat(android): migrate CI workflows to monorepo"`

1. Create `.github/workflows/lint-android.yml`
2. Create `.github/workflows/test-android.yml`
3. Delete `sdks/android/.github/workflows/claude-review.yml`
4. Delete `sdks/android/.github/workflows/triage.yml`
5. Delete `sdks/android/.github/workflows/docs.yml`

### PR 3: Documentation
Branch: `gt create -am "docs(android): update documentation for monorepo"`

1. Update root `README.md`
2. Update root `CLAUDE.md`
3. Rewrite `sdks/android/README.md`
4. Rewrite `sdks/android/CLAUDE.md`

## Workflow Notes

- Use Graphite for stacked PRs: PR1 → PR2 → PR3
- Do not submit or push code
- Three branches stacked on top of one another

## Future Work

- Investigate running Android emulators from Nix
