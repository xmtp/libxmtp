# Android SDK Monorepo Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate xmtp-android into the libxmtp monorepo with Nix-based builds, CI workflows, and updated documentation.

**Architecture:** Three stacked PRs using Graphite. PR1 sets up local builds with Nix derivations for Android bindings. PR2 migrates CI workflows. PR3 updates documentation.

**Tech Stack:** Nix (flakes, crane), Gradle, Kotlin, cargo-ndk, Graphite (gt)

---

## PR 1: Local Build Setup

**Branch:** `gt create -am "feat(android): local build setup with Nix bindings"`

### Task 1.1: Add shared depsOnly fileset to nix/lib/filesets.nix

**Files:**
- Modify: `nix/lib/filesets.nix`

**Step 1: Add depsOnly fileset**

Add after line 5 (after `src = ./../..;`):

```nix
  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  #
  # Used by both iOS and Android package derivations for consistent caching.
  depsOnly = lib.fileset.unions [
    (src + /Cargo.lock)
    (src + /.cargo/config.toml)
    # All Cargo.toml and build.rs files in the workspace
    (lib.fileset.fileFilter (file:
      file.name == "Cargo.toml" || file.name == "build.rs"
    ) src)
    # Files referenced by build scripts (e.g., include_bytes!, include_str!).
    # These are needed at dep-compilation time because build.rs runs then.
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (src + /crates/xmtp_db/migrations)
    (src + /crates/xmtp_proto/src/gen/proto_descriptor.bin)
  ];
```

**Step 2: Export depsOnly in the return set**

Change the final return from:
```nix
{
  inherit libraries binaries forCrate workspace;
}
```
To:
```nix
{
  inherit depsOnly libraries binaries forCrate workspace;
}
```

**Step 3: Verify syntax**

Run: `nix flake check --no-build 2>&1 | head -20`
Expected: No syntax errors related to filesets.nix

---

### Task 1.2: Update iOS derivation to use shared fileset

**Files:**
- Modify: `nix/package/ios.nix`

**Step 1: Replace depsFileset definition**

Find the `depsFileset = lib.fileset.toSource` block (around line 56-75) and replace with:

```nix
  # Shared filesets from nix/lib/filesets.nix
  filesets = xmtp.filesets { inherit lib craneLib; };

  # Narrow fileset for buildDepsOnly (shared with Android).
  # See nix/lib/filesets.nix for details on what's included.
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.depsOnly;
  };
```

**Step 2: Update bindingsFileset to use shared filesets**

Change:
```nix
    fileset = (xmtp.filesets { inherit lib craneLib; }).forCrate ./../../bindings/mobile;
```
To:
```nix
    fileset = filesets.forCrate ./../../bindings/mobile;
```

**Step 3: Verify iOS derivation still evaluates**

Run: `nix eval .#ios-libs.name 2>&1`
Expected: `"xmtpv3-ios-libs-<version>"` (on macOS) or error about darwin-only (on Linux)

---

### Task 1.3: Create nix/package/android.nix

**Files:**
- Create: `nix/package/android.nix`

**Step 1: Create the Android derivation file**

```nix
# Android cross-compilation package derivation.
# Builds shared libraries (.so) for 4 Android targets + Kotlin bindings, cacheable in Cachix.
#
# This file produces 6 derivations:
#   1-4. Per-target shared libraries:
#        xmtpv3-{aarch64-linux-android,armv7-linux-androideabi,x86_64-linux-android,i686-linux-android}
#   5.   Kotlin bindings:
#        xmtpv3-kotlin-bindings (runs uniffi-bindgen, outputs .kt file)
#   6.   Aggregate (symlinks all outputs):
#        xmtpv3-android-libs
#
# Unlike iOS, Android builds are pure - the Android NDK is fully packaged in nixpkgs.
{ lib
, zstd
, openssl
, sqlite
, pkg-config
, perl
, gnused
, craneLib
, xmtp
, stdenv
, androidenv
, ...
}:
let
  # Android target configuration
  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Map Rust targets to Android ABI names
  targetToAbi = {
    "aarch64-linux-android" = "arm64-v8a";
    "armv7-linux-androideabi" = "armeabi-v7a";
    "x86_64-linux-android" = "x86_64";
    "i686-linux-android" = "x86";
  };

  # Android SDK/NDK setup
  androidComposition = androidenv.composeAndroidPackages {
    platformVersions = [ "33" "34" ];
    platformToolsVersion = "35.0.2";
    buildToolsVersions = [ "30.0.3" ];
    includeNDK = true;
  };

  androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
  ndkVersion = builtins.head (lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"));
  ndkHome = "${androidHome}/ndk/${ndkVersion}";

  # Rust toolchain with Android cross-compilation targets (no dev tools needed for builds)
  rust-toolchain = xmtp.mkToolchain androidTargets [];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  # Shared filesets from nix/lib/filesets.nix
  filesets = xmtp.filesets { inherit lib craneLib; };

  # Narrow fileset for buildDepsOnly (shared with iOS)
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.depsOnly;
  };

  # Full fileset for buildPackage
  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.forCrate ./../../bindings/mobile;
  };

  # Common build arguments shared across all targets
  commonArgs = {
    src = depsFileset;
    strictDeps = true;
    nativeBuildInputs = [ pkg-config perl ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    hardeningDisable = [ "zerocallusedregs" ];

    # Android NDK environment
    ANDROID_HOME = androidHome;
    ANDROID_NDK_HOME = ndkHome;
    ANDROID_NDK_ROOT = ndkHome;
  };

  # Build shared library (.so) for a single Android target
  buildTarget = target:
    let
      abi = targetToAbi.${target};

      # Linker configuration for Android cross-compilation
      targetUpper = lib.toUpper (builtins.replaceStrings ["-"] ["_"] target);

      targetEnv = {
        "CARGO_TARGET_${targetUpper}_LINKER" = "${ndkHome}/toolchains/llvm/prebuilt/${if stdenv.isDarwin then "darwin-x86_64" else "linux-x86_64"}/bin/${target}31-clang";
        CARGO_BUILD_TARGET = target;
      };

      # Phase 1: Dep caching
      cargoArtifacts = rust.buildDepsOnly (commonArgs // targetEnv // {
        pname = "xmtpv3-deps-${target}";
        cargoExtraArgs = "--target ${target} -p xmtpv3";
      });
    in
    # Phase 2: Build project source
    rust.buildPackage (commonArgs // targetEnv // {
      inherit cargoArtifacts;
      pname = "xmtpv3-${target}";
      src = bindingsFileset;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../Cargo.toml;
      }) version;
      cargoExtraArgs = "--target ${target} -p xmtpv3";
      installPhaseCommand = ''
        mkdir -p $out/${abi}
        cp target/${target}/release/libxmtpv3.so $out/${abi}/libuniffi_xmtpv3.so
      '';
    });

  # Per-target derivations
  targets = lib.genAttrs androidTargets buildTarget;

  # Kotlin bindings derivation
  # Builds for the native host, then runs uniffi-bindgen to generate Kotlin bindings
  kotlinBindings = rust.buildPackage (commonArgs // {
    pname = "xmtpv3-kotlin-bindings";
    src = bindingsFileset;
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }) version;
    cargoArtifacts = rust.buildDepsOnly (commonArgs // {
      pname = "xmtpv3-kotlin-bindings-deps";
      cargoExtraArgs = "-p xmtpv3";
    });
    cargoExtraArgs = "-p xmtpv3";
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ gnused ];
    doNotPostBuildInstallCargoBinaries = true;
    installPhaseCommand = ''
      # Generate Kotlin bindings using uniffi-bindgen
      cargo run -p xmtpv3 --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
        --library target/release/libxmtpv3.${if stdenv.isDarwin then "dylib" else "so"} \
        --out-dir $TMPDIR/kotlin-out \
        --language kotlin

      mkdir -p $out/kotlin

      # Apply required sed replacements:
      # 1) Replace `return "xmtpv3"` with `return "uniffi_xmtpv3"` (library name fix)
      # 2) Replace `value.forEach { (k, v) ->` with `value.iterator().forEach { (k, v) ->`
      sed -i \
        -e 's/return "xmtpv3"/return "uniffi_xmtpv3"/' \
        -e 's/value\.forEach { (k, v) ->/value.iterator().forEach { (k, v) ->/g' \
        "$TMPDIR/kotlin-out/xmtpv3/xmtpv3.kt"

      cp $TMPDIR/kotlin-out/xmtpv3/xmtpv3.kt $out/kotlin/

      # Generate version file
      echo "Version: unknown" > $out/kotlin/libxmtp-version.txt
      echo "Date: $(date -u +%Y-%m-%d)" >> $out/kotlin/libxmtp-version.txt
    '';
  });

  # Aggregate derivation: combines all per-target libraries + Kotlin bindings
  aggregate = stdenv.mkDerivation {
    pname = "xmtpv3-android-libs";
    version = (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }).version;
    dontUnpack = true;
    installPhase = ''
      mkdir -p $out/jniLibs $out/kotlin
      ${lib.concatMapStringsSep "\n" (target:
        let abi = targetToAbi.${target}; in ''
        mkdir -p $out/jniLibs/${abi}
        ln -s ${targets.${target}}/${abi}/libuniffi_xmtpv3.so $out/jniLibs/${abi}/libuniffi_xmtpv3.so
      '') androidTargets}
      ln -s ${kotlinBindings}/kotlin/xmtpv3.kt $out/kotlin/xmtpv3.kt
      ln -s ${kotlinBindings}/kotlin/libxmtp-version.txt $out/kotlin/libxmtp-version.txt
    '';
  };

in
{
  inherit targets kotlinBindings aggregate;
}
```

**Step 2: Verify file was created**

Run: `ls -la nix/package/android.nix`
Expected: File exists with appropriate size (~5KB)

---

### Task 1.4: Add android-libs package to flake.nix

**Files:**
- Modify: `flake.nix`

**Step 1: Add android-libs to packages**

Find the `packages = {` block and add after `wasm-bindgen-cli`:

```nix
            # Android bindings (.so libraries + Kotlin bindings)
            android-libs = (pkgs.callPackage ./nix/package/android.nix {
              craneLib = crane.mkLib pkgs;
            }).aggregate;
```

**Step 2: Verify flake evaluates**

Run: `nix flake check --no-build 2>&1 | head -20`
Expected: No errors (warnings are OK)

**Step 3: Verify android-libs package is available**

Run: `nix eval .#android-libs.name 2>&1`
Expected: `"xmtpv3-android-libs-<version>"`

---

### Task 1.5: Create sdks/android/.envrc

**Files:**
- Create: `sdks/android/.envrc`

**Step 1: Create .envrc file**

```bash
use flake ../../#android --accept-flake-config
```

**Step 2: Verify file was created**

Run: `cat sdks/android/.envrc`
Expected: Shows the flake reference

---

### Task 1.6: Create sdks/android/dev/.setup

**Files:**
- Create: `sdks/android/dev/.setup`

**Step 1: Create .setup file**

```bash
#!/bin/bash
# Common setup for Android dev scripts
# Source this file, don't execute it directly

set -eou pipefail

ROOT="$(git rev-parse --show-toplevel)"
SDK_ROOT="${ROOT}/sdks/android"

# Re-exec in the android Nix shell if not already there
ensure_nix_shell() {
    if [[ -z "${IN_NIX_SHELL:-}" ]]; then
        exec nix develop "${ROOT}#android" --command "$0" "$@"
    fi
}
```

**Step 2: Verify file was created**

Run: `cat sdks/android/dev/.setup`
Expected: Shows the setup script content

---

### Task 1.7: Create sdks/android/dev/bindings

**Files:**
- Create: `sdks/android/dev/bindings`

**Step 1: Create bindings script**

```bash
#!/bin/bash
# Build Android bindings (.so libraries + Kotlin bindings) via Nix
source "$(dirname "$0")/.setup"

echo "Building Android bindings via Nix..."

# Build .so libs + Kotlin bindings via Nix (cached in Cachix)
nix build "${ROOT}#android-libs" --out-link "${SDK_ROOT}/.build/nix"

# Set up the bindings directory structure for Gradle
BINDINGS_DIR="${SDK_ROOT}/.build/bindings"
rm -rf "${BINDINGS_DIR}"
mkdir -p "${BINDINGS_DIR}/java"
mkdir -p "${BINDINGS_DIR}/jniLibs"

# Copy Kotlin bindings
cp "${SDK_ROOT}/.build/nix/kotlin/xmtpv3.kt" "${BINDINGS_DIR}/java/"
cp "${SDK_ROOT}/.build/nix/kotlin/libxmtp-version.txt" "${BINDINGS_DIR}/"

# Copy JNI libraries for each ABI
for abi in arm64-v8a armeabi-v7a x86 x86_64; do
    mkdir -p "${BINDINGS_DIR}/jniLibs/${abi}"
    cp "${SDK_ROOT}/.build/nix/jniLibs/${abi}/libuniffi_xmtpv3.so" "${BINDINGS_DIR}/jniLibs/${abi}/"
done

echo "Android bindings built successfully:"
echo "  Kotlin: ${BINDINGS_DIR}/java/xmtpv3.kt"
echo "  JNI libs: ${BINDINGS_DIR}/jniLibs/"
```

**Step 2: Make executable**

Run: `chmod +x sdks/android/dev/bindings`

**Step 3: Verify file is executable**

Run: `ls -la sdks/android/dev/bindings`
Expected: Shows `-rwxr-xr-x` permissions

---

### Task 1.8: Create sdks/android/dev/build

**Files:**
- Create: `sdks/android/dev/build`

**Step 1: Create build script**

```bash
#!/bin/bash
# Build the Android SDK
source "$(dirname "$0")/.setup"
ensure_nix_shell "$@"

# Ensure bindings are built
"${SDK_ROOT}/dev/bindings"

# Run Gradle build
cd "${SDK_ROOT}"
./gradlew build
```

**Step 2: Make executable**

Run: `chmod +x sdks/android/dev/build`

**Step 3: Verify file is executable**

Run: `ls -la sdks/android/dev/build`
Expected: Shows `-rwxr-xr-x` permissions

---

### Task 1.9: Update sdks/android/library/build.gradle

**Files:**
- Modify: `sdks/android/library/build.gradle`

**Step 1: Add sourceSets configuration**

Find the `lint {` block closing brace and add after it (but still inside `android {`):

```gradle
    // Include generated bindings from Nix build
    sourceSets {
        main {
            java.srcDirs += ["${rootProject.projectDir}/.build/bindings/java"]
            jniLibs.srcDirs += ["${rootProject.projectDir}/.build/bindings/jniLibs"]
        }
    }
```

**Step 2: Verify syntax**

Run: `cd sdks/android && ./gradlew help 2>&1 | tail -5`
Expected: Shows Gradle help output without build.gradle syntax errors

---

### Task 1.10: Update sdks/android/.gitignore

**Files:**
- Modify: `sdks/android/.gitignore`

**Step 1: Add Nix/direnv entries at top of file**

Add at the beginning of the file:

```
# Nix/direnv
.direnv/
.build/

```

**Step 2: Verify entries added**

Run: `head -5 sdks/android/.gitignore`
Expected: Shows the new Nix/direnv entries

---

### Task 1.11: Delete legacy files

**Files:**
- Delete: `sdks/android/script/`
- Delete: `sdks/android/libxmtp/`
- Delete: `dev/release-kotlin`

**Step 1: Delete legacy directories and files**

Run:
```bash
rm -rf sdks/android/script/
rm -rf sdks/android/libxmtp/
rm -f dev/release-kotlin
```

**Step 2: Verify deletions**

Run: `ls sdks/android/script 2>&1; ls sdks/android/libxmtp 2>&1; ls dev/release-kotlin 2>&1`
Expected: All three should show "No such file or directory"

---

### Task 1.12: Verify PR1 - Nix evaluation

**Step 1: Verify flake evaluates without errors**

Run: `nix flake check --no-build 2>&1 | grep -E "(error|warning:.*android)" | head -10`
Expected: No errors related to android

**Step 2: Verify android-libs package name**

Run: `nix eval .#android-libs.name`
Expected: `"xmtpv3-android-libs-<version>"`

---

### Task 1.13: Verify PR1 - Gradle configuration

**Step 1: Verify Gradle can parse build files**

Run: `cd sdks/android && ./gradlew projects 2>&1 | tail -10`
Expected: Shows project structure without errors

---

### Task 1.14: Create PR1 Graphite branch

**Step 1: Stage all changes**

Run: `git add -A`

**Step 2: Create Graphite branch**

Run: `gt create -am "feat(android): local build setup with Nix bindings"`
Expected: Branch created successfully

**Step 3: Verify branch created**

Run: `git branch --show-current`
Expected: Shows new branch name (not the original branch)

---

## PR 2: CI Workflows

**Branch:** `gt create -am "feat(android): migrate CI workflows to monorepo"`

### Task 2.1: Create .github/workflows/lint-android.yml

**Files:**
- Create: `.github/workflows/lint-android.yml`

**Step 1: Create lint workflow**

```yaml
name: Lint Android

on:
  push:
    branches: ["main"]
    paths: ["sdks/android/**"]
  pull_request:
    paths: ["sdks/android/**"]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - name: Run Spotless Check
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android spotlessCheck --continue
      - name: Run Android Lint
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android :library:lintDebug
```

**Step 2: Verify file created**

Run: `cat .github/workflows/lint-android.yml | head -10`
Expected: Shows workflow header

---

### Task 2.2: Create .github/workflows/test-android.yml

**Files:**
- Create: `.github/workflows/test-android.yml`

**Step 1: Create test workflow**

```yaml
name: Android Tests

on:
  push:
    branches: ["main"]
    paths: ["sdks/android/**", "bindings/mobile/**", "crates/**"]
  pull_request:
    paths: ["sdks/android/**", "bindings/mobile/**", "crates/**"]

concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  unit-tests:
    name: Unit Tests
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
      - name: Build bindings
        run: nix develop .#android --command ./sdks/android/dev/bindings
      - name: Start backend
        run: ./dev/up
      - name: Run unit tests
        run: nix develop .#android --command ./sdks/android/gradlew -p sdks/android library:testDebug

  integration-tests:
    name: Integration Tests
    runs-on: warp-ubuntu-latest-x64-16x
    # Note: Investigate running emulators from Nix in future work
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
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
          disable-animations: true
          emulator-options: -no-snapshot-save -no-window -gpu swiftshader_indirect -noaudio -no-boot-anim -memory 4096 -partition-size 8192
          script: nix develop .#android --command ./sdks/android/gradlew -p sdks/android connectedCheck --continue
```

**Step 2: Verify file created**

Run: `cat .github/workflows/test-android.yml | head -10`
Expected: Shows workflow header

---

### Task 2.3: Delete obsolete Android workflows

**Files:**
- Delete: `sdks/android/.github/workflows/claude-review.yml`
- Delete: `sdks/android/.github/workflows/triage.yml`
- Delete: `sdks/android/.github/workflows/docs.yml`

**Step 1: Delete obsolete workflows**

Run:
```bash
rm -f sdks/android/.github/workflows/claude-review.yml
rm -f sdks/android/.github/workflows/triage.yml
rm -f sdks/android/.github/workflows/docs.yml
```

**Step 2: Verify deletions**

Run: `ls sdks/android/.github/workflows/`
Expected: Shows only `dev-release.yml`, `lint.yml`, `release.yml`, `test.yml`

---

### Task 2.4: Verify PR2 - Workflow YAML syntax

**Step 1: Verify lint-android.yml syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/lint-android.yml'))" && echo "Valid YAML"`
Expected: `Valid YAML`

**Step 2: Verify test-android.yml syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test-android.yml'))" && echo "Valid YAML"`
Expected: `Valid YAML`

---

### Task 2.5: Create PR2 Graphite branch

**Step 1: Stage all changes**

Run: `git add -A`

**Step 2: Create Graphite branch**

Run: `gt create -am "feat(android): migrate CI workflows to monorepo"`
Expected: Branch created successfully

**Step 3: Verify branch stacking**

Run: `gt log short | head -5`
Expected: Shows PR2 branch stacked on PR1 branch

---

## PR 3: Documentation

**Branch:** `gt create -am "docs(android): update documentation for monorepo"`

### Task 3.1: Update root README.md

**Files:**
- Modify: `README.md`

**Step 1: Find SDKs section and add Android**

Look for any existing SDK references or add a new section. Add:

```markdown
## SDKs

- [iOS SDK](sdks/ios/) - Swift SDK for iOS/macOS
- [Android SDK](sdks/android/) - Kotlin SDK for Android
```

**Step 2: Verify changes**

Run: `grep -A2 "## SDKs" README.md || grep -i android README.md`
Expected: Shows Android SDK reference

---

### Task 3.2: Update root CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add Android section to Development Commands**

Find the development commands section and add:

```markdown
### Android SDK

```bash
nix develop .#android         # Enter Android development shell
./sdks/android/dev/bindings   # Build Android bindings via Nix
./sdks/android/dev/build      # Build the full Android SDK
```
```

**Step 2: Verify changes**

Run: `grep -A5 "Android SDK" CLAUDE.md`
Expected: Shows Android commands

---

### Task 3.3: Rewrite sdks/android/README.md

**Files:**
- Modify: `sdks/android/README.md`

**Step 1: Update README for monorepo**

Replace the content with updated version that:
- Updates badges to point to monorepo workflows
- Updates development setup for Nix/direnv
- Updates build commands for dev/ scripts
- Keeps Maven Central install instructions
- Keeps API docs links

(Full content to be written based on existing README structure)

**Step 2: Verify no broken internal links**

Run: `grep -E "\[.*\]\(\./" sdks/android/README.md`
Expected: All relative links should still be valid

---

### Task 3.4: Rewrite sdks/android/CLAUDE.md

**Files:**
- Modify: `sdks/android/CLAUDE.md`

**Step 1: Update CLAUDE.md for monorepo**

Update to reflect:
- Now part of libxmtp monorepo
- Nix devShell and direnv setup
- New dev/ scripts (bindings, build)
- Remove references to deleted directories (script/, libxmtp/)
- Reference bindings/mobile/ for native bindings

**Step 2: Verify no references to deleted paths**

Run: `grep -E "(script/|libxmtp/)" sdks/android/CLAUDE.md`
Expected: No output (no references to deleted paths)

---

### Task 3.5: Verify PR3 - Documentation links

**Step 1: Check for broken relative links in README**

Run: `cd sdks/android && grep -oE "\[.*\]\([^)]+\)" README.md | grep -E "^\[.*\]\(\." | while read link; do path=$(echo "$link" | sed 's/.*(\(.*\))/\1/'); [ -e "$path" ] || echo "Broken: $path"; done`
Expected: No broken links

---

### Task 3.6: Create PR3 Graphite branch

**Step 1: Stage all changes**

Run: `git add -A`

**Step 2: Create Graphite branch**

Run: `gt create -am "docs(android): update documentation for monorepo"`
Expected: Branch created successfully

**Step 3: Verify full stack**

Run: `gt log short`
Expected: Shows 3 branches stacked: PR3 -> PR2 -> PR1 -> base

---

## Final Verification

### Task F.1: Verify Graphite stack

**Step 1: Check stack structure**

Run: `gt log short`
Expected: Shows 3 branches in correct order

**Step 2: Verify no uncommitted changes**

Run: `git status --short`
Expected: Clean working directory (no output)

---

## Notes

- Do NOT run `gt submit` or `git push`
- The Nix build (`nix build .#android-libs`) is expensive and should only be run for final verification if needed
- All verification steps use quick checks (syntax, evaluation) rather than full builds
- Future work: Investigate running Android emulators from Nix
