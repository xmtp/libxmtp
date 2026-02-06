# iOS Nix Derivation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Cache iOS cross-compilation static libraries and Swift bindings in Cachix via Nix derivations, replacing the CI build matrix with `nix build .#ios-libs`.

**Architecture:** Per-target crane derivations (impure, `__noChroot`) for 4 static library targets, a pure derivation for Swift binding generation, and an aggregate combining all outputs. Makefile updated to read from `build/nix/` symlink, write lipo/xcframework to `build/`. CI collapses 5 jobs into 1.

**Tech Stack:** Nix (crane, fenix, flake-parts), Rust cross-compilation, Make, GitHub Actions, Cachix

**Design doc:** `docs/plans/2026-02-03-ios-nix-derivation-design.md`

---

### Task 1: Create `nix/package/ios.nix` — per-target static library derivations

**Files:**
- Create: `nix/package/ios.nix`

**Reference files (read-only):**
- `nix/package/wasm.nix` — pattern to follow for crane setup
- `nix/ios.nix` — environment variables to replicate
- `nix/lib/mkToolchain.nix` — toolchain constructor
- `nix/lib/filesets.nix` — source filtering

**Step 1: Write `nix/package/ios.nix` with per-target builds**

The file structure follows `wasm.nix`. Key differences: multiple `CARGO_BUILD_TARGET` values, impure builds via `__noChroot`, and iOS-specific env vars from the shell hook.

```nix
{ lib
, fenix
, zstd
, openssl
, sqlite
, pkg-config
, craneLib
, xmtp
, stdenv
, ...
}:
let
  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Pinned Rust Version with iOS targets
  rust-toolchain = xmtp.mkToolchain iosTargets [ "clippy-preview" "rustfmt-preview" ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  libraryFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).libraries;
  };

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).forCrate ./../../bindings/mobile;
  };

  # Common Xcode/iOS environment variables (mirrors nix/ios.nix shellHook)
  developerDir = "/Applications/Xcode.app/Contents/Developer";
  iosSdk = "${developerDir}/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk";
  iosSimSdk = "${developerDir}/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk";

  iosEnv = {
    DEVELOPER_DIR = developerDir;
    CC_aarch64_apple_ios = "/usr/bin/clang";
    CXX_aarch64_apple_ios = "/usr/bin/clang++";
    CC_aarch64_apple_ios_sim = "/usr/bin/clang";
    CXX_aarch64_apple_ios_sim = "/usr/bin/clang++";
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER = "/usr/bin/clang";
    CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER = "/usr/bin/clang";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios = "--target=arm64-apple-ios --sysroot=${iosSdk}";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim = "--target=arm64-apple-ios-simulator --sysroot=${iosSimSdk}";
  };

  commonArgs = {
    src = libraryFileset;
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    # For iOS cross-compilation, openssl must be vendored (builds from source per target)
    # Do NOT set OPENSSL_NO_VENDOR
    hardeningDisable = [ "zerocallusedregs" ];
  };

  # Build a static library for a single target
  buildTarget = target:
    let
      targetEnv = iosEnv // {
        CARGO_BUILD_TARGET = target;
      };

      # Dep caching: only rebuilds when Cargo.lock changes
      cargoArtifacts = rust.buildDepsOnly (targetEnv // commonArgs // {
        pname = "xmtpv3-deps-${target}";
        # Impure: needs Xcode SDK for bindgen during dep compilation
        __noChroot = true;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
      });
    in
    rust.buildPackage (targetEnv // commonArgs // {
      inherit cargoArtifacts;
      __noChroot = true;
      pname = "xmtpv3-${target}";
      src = bindingsFileset;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../Cargo.toml;
      }) version;
      cargoExtraArgs = "--target ${target} -p xmtpv3";
      installPhaseCommand = ''
        mkdir -p $out/${target}
        cp target/${target}/release/libxmtpv3.a $out/${target}/
      '';
    });

  # Per-target derivations
  targets = lib.genAttrs iosTargets buildTarget;

  # Swift bindings derivation (pure — native host build only)
  swiftBindings = rust.buildPackage (commonArgs // {
    pname = "xmtpv3-swift-bindings";
    src = bindingsFileset;
    inherit (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }) version;
    cargoArtifacts = rust.buildDepsOnly (commonArgs // {
      pname = "xmtpv3-swift-bindings-deps";
      cargoExtraArgs = "-p xmtpv3";
    });
    cargoExtraArgs = "-p xmtpv3";
    buildPhaseCargoCommand = ''
      cargo build --release -p xmtpv3
    '';
    installPhaseCommand = ''
      # Generate Swift bindings using uniffi-bindgen
      cargo run --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
        --library target/release/libxmtpv3.a \
        --out-dir $TMPDIR/swift-out \
        --language swift

      # Organize into expected directory structure
      mkdir -p $out/swift/include/libxmtp
      cp $TMPDIR/swift-out/xmtpv3.swift $out/swift/
      mv $TMPDIR/swift-out/xmtpv3FFI.h $out/swift/include/libxmtp/
      mv $TMPDIR/swift-out/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap
    '';
  });

  # Aggregate: combines all targets + swift bindings
  aggregate = stdenv.mkDerivation {
    pname = "xmtpv3-ios-libs";
    version = (rust.crateNameFromCargoToml {
      cargoToml = ./../../Cargo.toml;
    }).version;
    dontUnpack = true;
    installPhase = ''
      mkdir -p $out/swift
      ${lib.concatMapStringsSep "\n" (target: ''
        mkdir -p $out/${target}
        ln -s ${targets.${target}}/${target}/libxmtpv3.a $out/${target}/libxmtpv3.a
      '') iosTargets}
      ln -s ${swiftBindings}/swift/xmtpv3.swift $out/swift/xmtpv3.swift
      ln -s ${swiftBindings}/swift/include $out/swift/include
    '';
  };

in
{
  inherit targets swiftBindings aggregate;
}
```

**Step 2: Verify the file parses**

Run: `nix eval .#ios-libs --apply 'x: builtins.typeOf x' 2>&1 | head -20`

If there are Nix parse errors, fix them. This does NOT build anything — it just checks the Nix expression is valid.

**Step 3: Commit**

```bash
git add nix/package/ios.nix
git commit -m "feat: add iOS static library Nix derivations"
```

---

### Task 2: Wire `ios-libs` into `flake.nix`

**Files:**
- Modify: `flake.nix:58-63` (the `packages` section and darwin `devShells`)

**Step 1: Add `packages.ios-libs` to the flake**

In `flake.nix`, inside the `perSystem` block, add the ios-libs package alongside the existing wasm-bindings package. It must be inside `lib.optionalAttrs pkgs.stdenv.isDarwin` since it only works on macOS.

Change this section (around line 62-63):

```nix
          packages.wasm-bindings = (pkgs.callPackage ./nix/package/wasm.nix { craneLib = crane.mkLib pkgs; }).bin;
          packages.wasm-bindgen-cli = pkgs.callPackage ./nix/lib/packages/wasm-bindgen-cli.nix { };
```

To:

```nix
          packages.wasm-bindings = (pkgs.callPackage ./nix/package/wasm.nix { craneLib = crane.mkLib pkgs; }).bin;
          packages.wasm-bindgen-cli = pkgs.callPackage ./nix/lib/packages/wasm-bindgen-cli.nix { };
          packages = lib.optionalAttrs pkgs.stdenv.isDarwin {
            ios-libs = (pkgs.callPackage ./nix/package/ios.nix { craneLib = crane.mkLib pkgs; }).aggregate;
          };
```

**Important:** The `packages` attribute set in flake-parts merges, so the `lib.optionalAttrs` block adds `ios-libs` alongside the existing packages only on Darwin. Verify this works by checking flake-parts docs — if it doesn't merge correctly, use the pattern:

```nix
          packages = {
            wasm-bindings = ...;
            wasm-bindgen-cli = ...;
          } // lib.optionalAttrs pkgs.stdenv.isDarwin {
            ios-libs = (pkgs.callPackage ./nix/package/ios.nix { craneLib = crane.mkLib pkgs; }).aggregate;
          };
```

**Step 2: Verify the flake evaluates**

Run: `nix flake show 2>&1 | grep ios-libs`

Expected: A line showing `ios-libs` under `packages.aarch64-darwin`.

**Step 3: Commit**

```bash
git add flake.nix
git commit -m "feat: expose ios-libs package in flake"
```

---

### Task 3: Build and verify the Nix derivation

This is the critical test. The first build will be slow (full cross-compilation of all targets). Subsequent builds will be fast.

**Step 1: Build the derivation**

Run:
```bash
nix build .#ios-libs --out-link bindings/mobile/build/nix 2>&1
```

This will take a long time on first run. Watch for errors. Common issues:
- Missing env vars → check that `iosEnv` in `ios.nix` matches the shell hook in `nix/ios.nix`
- Xcode SDK not found → verify `/Applications/Xcode.app` exists
- Cargo can't find crate → check `cargoExtraArgs` and fileset filtering
- `__noChroot` not recognized → may need `--option sandbox false` on the command line instead

**Step 2: Verify output structure**

Run:
```bash
ls -la bindings/mobile/build/nix/
ls -la bindings/mobile/build/nix/aarch64-apple-ios/
ls -la bindings/mobile/build/nix/swift/
ls -la bindings/mobile/build/nix/swift/include/libxmtp/
file bindings/mobile/build/nix/aarch64-apple-ios/libxmtpv3.a
```

Expected:
- 4 target directories each containing `libxmtpv3.a`
- `swift/xmtpv3.swift` exists
- `swift/include/libxmtp/xmtpv3FFI.h` exists
- `swift/include/libxmtp/module.modulemap` exists
- `file` reports a static library (ar archive)

**Step 3: Verify cache hit**

Run:
```bash
nix build .#ios-libs --out-link bindings/mobile/build/nix 2>&1
```

Expected: Completes near-instantly (no compilation output).

**Step 4: Commit (no file changes, but tag this milestone)**

No commit needed — no source files changed. Move on.

---

### Task 4: Update Makefile `lipo` and `framework` targets

**Files:**
- Modify: `bindings/mobile/Makefile:18-19,65-68,77-89`

**Step 1: Add `NIX_OUT` variable and update `lipo`**

At the top of the Makefile, after line 19 (`IOS_SDK_SWIFT_DIR`), add:

```makefile
NIX_OUT ?= build/nix
```

Update the `lipo` target (lines 65-68) to read from `$(NIX_OUT)`:

```makefile
lipo:
	mkdir -p build/lipo_macos build/lipo_ios_sim
	lipo -create -output build/lipo_ios_sim/$(LIB) $(foreach arch,$(ARCHS_IOS),$(wildcard $(NIX_OUT)/$(arch)/$(LIB)))
	lipo -create -output build/lipo_macos/$(LIB) $(foreach arch,$(ARCHS_MAC),$(wildcard $(NIX_OUT)/$(arch)/$(LIB)))
```

**Step 2: Update the `framework` target**

Update the `framework` target (lines 77-89) to:
- Read headers from `$(NIX_OUT)/swift/include/libxmtp/`
- Read `.a` files from `$(NIX_OUT)/` directories
- Output xcframework to `build/swift/LibXMTPSwiftFFI.xcframework`
- Copy swift file to `$(IOS_SDK_SWIFT_DIR)`

```makefile
framework: lipo
	mkdir -p build/swift
	mkdir -p $(IOS_SDK_SWIFT_DIR)
	rm -rf build/swift/LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library $(NIX_OUT)/aarch64-apple-ios/$(LIB) \
		-headers $(NIX_OUT)/swift/include/libxmtp/ \
		-library build/lipo_ios_sim/$(LIB) \
		-headers $(NIX_OUT)/swift/include/libxmtp/ \
		-library build/lipo_macos/$(LIB) \
		-headers $(NIX_OUT)/swift/include/libxmtp/ \
		-output build/swift/LibXMTPSwiftFFI.xcframework
	cp $(NIX_OUT)/swift/xmtpv3.swift $(IOS_SDK_SWIFT_DIR)/xmtpv3.swift
```

**Step 3: Update the `local` target to set `NIX_OUT=build`**

The `local` target builds everything from cargo into `build/<target>/`. For this path, `NIX_OUT` must point to `build` (not `build/nix`). The per-target build rules already put `.a` files in `build/<target>/`, and `bindgenstatic swift` puts Swift bindings in `build/swift/static/`. Update `local`:

```makefile
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework
```

This already works because the per-target rules write to `build/<target>/` and the `swift` target writes headers to `build/swift/static/include/libxmtp/`. But `lipo` and `framework` now read from `$(NIX_OUT)` which defaults to `build/nix`. So we need `local` to override `NIX_OUT`:

Replace the `local` target with:

```makefile
local: NIX_OUT = build
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo-local framework-local
```

Wait — this is getting complex. Simpler approach: since the per-target cargo builds already put `.a` files in `build/<target>/`, and `bindgenstatic swift` puts Swift output in `build/swift/static/`, the `local` target just needs to invoke `lipo` and `framework` with the right source directory. Use Make's target-specific variable:

```makefile
local: NIX_OUT = build
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework
```

But the Swift bindings from `bindgenstatic swift` go to `build/swift/static/` (with headers at `build/swift/static/include/libxmtp/`), while the Nix derivation puts them at `$(NIX_OUT)/swift/include/libxmtp/`. For `local` we need `NIX_OUT` to look at `build/swift/static` for Swift bindings but `build` for `.a` files. These are in the same tree when `NIX_OUT=build` — `.a` at `build/<target>/` and headers at `build/swift/static/include/libxmtp/`.

So: set `NIX_OUT=build` for `.a` files in `lipo`, and use a separate `SWIFT_OUT` variable for Swift binding locations:

Add near the top:
```makefile
NIX_OUT ?= build/nix
SWIFT_OUT ?= $(NIX_OUT)/swift
```

For the `local` target, override both:
```makefile
local: NIX_OUT = build
local: SWIFT_OUT = build/swift/static
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework
```

Update `framework` to use `$(SWIFT_OUT)`:
```makefile
framework: lipo
	mkdir -p build/swift
	mkdir -p $(IOS_SDK_SWIFT_DIR)
	rm -rf build/swift/LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library $(NIX_OUT)/aarch64-apple-ios/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_ios_sim/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_macos/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-output build/swift/LibXMTPSwiftFFI.xcframework
	cp $(SWIFT_OUT)/xmtpv3.swift $(IOS_SDK_SWIFT_DIR)/xmtpv3.swift
```

**Step 4: Verify the Nix path works**

Run (from ios dev shell or after `nix build .#ios-libs --out-link bindings/mobile/build/nix`):
```bash
cd bindings/mobile
nix develop ../../#ios --command make lipo framework
```

Expected: xcframework built at `bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework`.

Verify:
```bash
ls build/swift/LibXMTPSwiftFFI.xcframework/
```
Expected: `ios-arm64/`, `ios-arm64-simulator/` (or `ios-arm64_x86_64-simulator`), `macos-arm64_x86_64/`, `Info.plist`.

**Step 5: Commit**

```bash
git add bindings/mobile/Makefile
git commit -m "feat: update Makefile to read from NIX_OUT for lipo and framework"
```

---

### Task 5: Update `Package.swift`

**Files:**
- Modify: `Package.swift:12-14,37-39`

**Step 1: Update the local binary path**

Change line 13 from:
```swift
	atPath: "\(thisPackagePath)/sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
```
to:
```swift
	atPath: "\(thisPackagePath)/bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework"
```

Change line 39 from:
```swift
				path: "sdks/ios/.build/LibXMTPSwiftFFI.xcframework"
```
to:
```swift
				path: "bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework"
```

**Step 2: Verify Swift can find the xcframework**

Run (assuming Task 4's framework was built):
```bash
swift build 2>&1 | tail -5
```

Expected: Build succeeds (or at least finds `LibXMTPSwiftFFI` — full build depends on network for other dependencies).

**Step 3: Commit**

```bash
git add Package.swift
git commit -m "feat: update Package.swift xcframework path to bindings/mobile/build"
```

---

### Task 6: Update `sdks/ios/dev/build` and `sdks/ios/dev/bindings`

**Files:**
- Modify: `sdks/ios/dev/build`
- Modify: `sdks/ios/dev/bindings`

**Step 1: Rewrite `sdks/ios/dev/build`**

```bash
#!/bin/bash
source "$(dirname "$0")/.setup"

# Build static libs + Swift bindings via Nix (cached in Cachix)
nix build "${ROOT}#ios-libs" --out-link "${ROOT}/bindings/mobile/build/nix"

# Enter Nix shell for xcframework assembly (needs Xcode)
ensure_nix_shell "$@"

# Assemble xcframework from Nix-built artifacts
cd "${ROOT}/bindings/mobile"
make lipo framework

# Build Swift package
cd "${ROOT}"
swift build
```

**Step 2: Rewrite `sdks/ios/dev/bindings`**

```bash
#!/bin/bash
source "$(dirname "$0")/.setup"

# Build static libs + Swift bindings via Nix (cached in Cachix)
nix build "${ROOT}#ios-libs" --out-link "${ROOT}/bindings/mobile/build/nix"

# Enter Nix shell for xcframework assembly (needs Xcode)
ensure_nix_shell "$@"

# Assemble xcframework from Nix-built artifacts
cd "${ROOT}/bindings/mobile"
make lipo framework
```

**Step 3: Verify the dev script end-to-end**

Run:
```bash
./sdks/ios/dev/build
```

Expected: Completes successfully. The `nix build` step should be near-instant (cache hit from Task 3), then lipo + framework run quickly, then `swift build` succeeds.

**Step 4: Commit**

```bash
git add sdks/ios/dev/build sdks/ios/dev/bindings
git commit -m "feat: update iOS dev scripts to use nix build for caching"
```

---

### Task 7: Update `.github/workflows/release-ios.yml`

**Files:**
- Modify: `.github/workflows/release-ios.yml`

**Step 1: Replace `build`, `generate-swift-bindings`, and `package` jobs with `build-and-package`**

Delete the `build` job (lines 57-89), `generate-swift-bindings` job (lines 91-113), and `package` job (lines 115-165).

Replace with a single `build-and-package` job:

```yaml
  build-and-package:
    needs: [compute-version]
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
      - uses: cachix/install-nix-action@v31
        with:
          extra_nix_config: |
            access-tokens = github.com=${{ secrets.GITHUB_TOKEN }}
      - uses: DeterminateSystems/magic-nix-cache-action@v13
      - uses: cachix/cachix-action@v16
        with:
          name: xmtp
          authToken: '${{ secrets.CACHIX_AUTH_TOKEN }}'
      - name: Build iOS libs
        run: nix build .#ios-libs --out-link bindings/mobile/build/nix
      - name: Build xcframework
        run: |
          nix develop .#ios --command make -C bindings/mobile lipo framework
      - name: Package zip
        working-directory: bindings/mobile
        run: |
          mkdir -p Sources/LibXMTP
          cp build/nix/swift/xmtpv3.swift Sources/LibXMTP/
          cp ../../LICENSE ./LICENSE
          zip -r LibXMTPSwiftFFI.zip \
            Sources \
            build/swift/LibXMTPSwiftFFI.xcframework \
            LICENSE
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
          SHA=$(git rev-parse --short=7 HEAD)
          TAG="libxmtp-ios-${SHA}"
          ARTIFACT_URL="https://github.com/${{ github.repository }}/releases/download/${TAG}/LibXMTPSwiftFFI.zip"

          if gh release view "$TAG" &>/dev/null; then
            echo "Release $TAG already exists, skipping creation"
          else
            gh release create "$TAG" \
              --title "iOS $VERSION - libxmtp binaries" \
              --notes "Intermediate artifact release for iOS SDK $VERSION" \
              --prerelease \
              LibXMTPSwiftFFI.zip
          fi

          echo "artifact-url=$ARTIFACT_URL" >> "$GITHUB_OUTPUT"
```

**Step 2: Update `publish` job dependency**

Change the `publish` job's `needs` from:
```yaml
    needs: [compute-version, package]
```
to:
```yaml
    needs: [compute-version, build-and-package]
```

And update the output references from `package` to `build-and-package`:
- `${{ needs.package.outputs.artifact-url }}` → `${{ needs.build-and-package.outputs.artifact-url }}`
- `${{ needs.package.outputs.checksum }}` → `${{ needs.build-and-package.outputs.checksum }}`

**Step 3: Verify YAML is valid**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release-ios.yml'))" 2>&1
```

Expected: No output (valid YAML).

**Step 4: Commit**

```bash
git add .github/workflows/release-ios.yml
git commit -m "feat: replace CI build matrix with nix build for iOS release"
```

---

### Task 8: Add `build/nix` to `.gitignore`

**Files:**
- Modify: `bindings/mobile/.gitignore`

**Step 1: Check current gitignore contents and add `nix` symlink**

Read `bindings/mobile/.gitignore`, then add `build/nix` if not already present. The `nix build --out-link` creates a symlink that should not be committed.

Run:
```bash
cat bindings/mobile/.gitignore
```

Then add a line for the nix output symlink:
```
build/nix
```

**Step 2: Commit**

```bash
git add bindings/mobile/.gitignore
git commit -m "chore: gitignore nix build output symlink"
```

---

### Task 9: Verification — Nix flow

**Step 1: Clean build directory and rebuild via Nix**

```bash
rm -rf bindings/mobile/build
nix build .#ios-libs --out-link bindings/mobile/build/nix
```

Verify all expected files:
```bash
test -f bindings/mobile/build/nix/aarch64-apple-ios/libxmtpv3.a
test -f bindings/mobile/build/nix/aarch64-apple-ios-sim/libxmtpv3.a
test -f bindings/mobile/build/nix/x86_64-apple-darwin/libxmtpv3.a
test -f bindings/mobile/build/nix/aarch64-apple-darwin/libxmtpv3.a
test -f bindings/mobile/build/nix/swift/xmtpv3.swift
test -f bindings/mobile/build/nix/swift/include/libxmtp/xmtpv3FFI.h
test -f bindings/mobile/build/nix/swift/include/libxmtp/module.modulemap
echo "All Nix output files present"
```

**Step 2: Assemble xcframework**

```bash
nix develop .#ios --command make -C bindings/mobile lipo framework
ls bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework/
```

Expected: 3 platform directories + Info.plist.

**Step 3: Swift build**

```bash
swift build 2>&1 | tail -5
```

Expected: Build succeeds.

**Step 4: Full dev script**

```bash
rm -rf bindings/mobile/build
./sdks/ios/dev/build
```

Expected: Completes successfully end-to-end.

**Step 5: Cache hit**

```bash
nix build .#ios-libs --out-link bindings/mobile/build/nix 2>&1
```

Expected: Near-instant, no compilation.

---

### Task 10: Verification — pure cargo flow

**Step 1: Build via `make local`**

```bash
rm -rf bindings/mobile/build
nix develop .#ios --command bash -c "cd bindings/mobile && make local"
```

Expected: Builds all 4 targets from source, generates Swift bindings, runs lipo, produces xcframework at `bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework`.

Verify:
```bash
ls bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework/
```

**Step 2: Individual target build**

```bash
nix develop .#ios --command bash -c "cd bindings/mobile && IPHONEOS_DEPLOYMENT_TARGET=14 cargo build --target aarch64-apple-ios-sim --release"
```

Expected: Compiles successfully.

**Step 3: Swift test smoke check**

```bash
swift test --filter XMTPTests.ClientTests 2>&1 | tail -10
```

Expected: Test file compiles and links against the xcframework (tests may fail if no backend is running — the key check is that the xcframework links correctly, not that tests pass).
