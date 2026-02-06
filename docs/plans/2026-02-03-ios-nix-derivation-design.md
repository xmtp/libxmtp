# iOS Bindings Nix Derivation

Cache the expensive iOS cross-compilation in Cachix by building static libraries and Swift bindings as Nix derivations.

## Goals

- Cache compiled iOS static libraries (`.a` files) in Cachix
- Replace the CI build matrix with a single `nix build` command
- Provide a one-command dev experience via `sdks/ios/dev/build`
- Keep xcframework assembly (lipo + xcodebuild) in the Makefile (fast, needs Xcode)

## Derivation Architecture

### File: `nix/package/ios.nix`

Follows the existing `nix/package/wasm.nix` pattern using crane.

**6 derivations:**

| Derivation | Pure? | Output |
|---|---|---|
| `ios-aarch64` | No (needs Xcode SDK) | `aarch64-apple-ios/libxmtpv3.a` |
| `ios-aarch64-sim` | No (needs Xcode SDK) | `aarch64-apple-ios-sim/libxmtpv3.a` |
| `macos-x86_64` | No (needs Xcode SDK) | `x86_64-apple-darwin/libxmtpv3.a` |
| `macos-aarch64` | No (needs Xcode SDK) | `aarch64-apple-darwin/libxmtpv3.a` |
| `ios-swift-bindings` | Yes | `swift/xmtpv3.swift`, `swift/include/libxmtp/{xmtpv3FFI.h, module.modulemap}` |
| `ios-libs` (aggregate) | Yes | Combines all above into one output |

**Per-target derivations** use:
- `crane.buildDepsOnly` for dependency caching (keyed on `Cargo.lock`)
- `crane.buildPackage` for the final build
- `__noChroot = true` for Xcode SDK access
- Same environment variables as the current `ios.nix` shell hook (`DEVELOPER_DIR`, `CC_*`, `CARGO_TARGET_*_LINKER`, `BINDGEN_EXTRA_CLANG_ARGS_*`)
- Source filtered via existing `filesets.forCrate ./../../bindings/mobile`
- `cargo build --release --target <target> --manifest-path bindings/mobile/Cargo.toml`

**Swift bindings derivation** is pure (native host build only):
- `cargo build --release -p xmtpv3` (native, no `--target`)
- `cargo run --bin ffi-uniffi-bindgen --release --features uniffi/cli generate --library <lib> --out-dir <out> --language swift`
- Reorganizes output into header/modulemap directory structure

**Aggregate output structure:**
```
$out/
  aarch64-apple-ios/libxmtpv3.a
  aarch64-apple-ios-sim/libxmtpv3.a
  x86_64-apple-darwin/libxmtpv3.a
  aarch64-apple-darwin/libxmtpv3.a
  swift/
    xmtpv3.swift
    include/libxmtp/
      xmtpv3FFI.h
      module.modulemap
```

### Flake integration

In `flake.nix`, expose the aggregate as `packages.ios-libs` (darwin-only):

```nix
packages = lib.optionalAttrs pkgs.stdenv.isDarwin {
  ios-libs = (pkgs.callPackage ./nix/package/ios.nix { craneLib = crane.mkLib pkgs; }).aggregate;
};
```

## Dev Shell & Script Integration

### `sdks/ios/dev/build`

Single path, no flags:

1. `nix build .#ios-libs --out-link bindings/mobile/build/nix` (cache hit or rebuild as needed)
2. Enter ios dev shell
3. `make lipo framework` (reads .a files from `build/nix/`, writes lipo + xcframework to `build/`)
4. `swift build`

### `sdks/ios/dev/bindings`

Same as above, minus `swift build`.

### Developer workflow

- **Not changing Rust code:** `nix build` is a cache hit, entire build is near-instant
- **Changing Rust code:** `nix build` rebuilds affected targets, deps are still cached if `Cargo.lock` unchanged
- **Iterating on a single target:** Use `cargo build --target <target>` directly in the ios dev shell (standard inner dev loop, unaffected by this change)

## Makefile Changes

### Source directory

Add a `NIX_OUT` variable defaulting to `build/nix` that the `lipo` and `framework` targets use to locate .a files and Swift bindings:
- `lipo`: read from `$(NIX_OUT)/<target>/libxmtpv3.a` instead of `build/<target>/libxmtpv3.a`
- `framework`: read headers from `$(NIX_OUT)/swift/include/libxmtp/` instead of `build/swift/static/include/libxmtp/`
- `framework`: output xcframework to `build/swift/LibXMTPSwiftFFI.xcframework`
- `framework`: copy `$(NIX_OUT)/swift/xmtpv3.swift` to `$(IOS_SDK_SWIFT_DIR)`

### `Package.swift`

Update the local binary check from:
```swift
sdks/ios/.build/LibXMTPSwiftFFI.xcframework
```
to:
```swift
bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework
```

## CI Integration

### `.github/workflows/release-ios.yml`

**Deleted jobs:** `build` (4-target matrix), `generate-swift-bindings`

**New combined job: `build-and-package`**

```
build-and-package:
  needs: [compute-version]
  runs-on: warp-macos-15-arm64-12x
  outputs:
    artifact-url, checksum (same as current package job)
  steps:
    - checkout
    - install nix
    - cachix/cachix-action (pull + push, xmtp cache)
    - nix build .#ios-libs --out-link bindings/mobile/build/nix
    - nix develop .#ios --command make -C bindings/mobile lipo framework
    - build zip from new paths:
        mkdir -p bindings/mobile/Sources/LibXMTP
        cp bindings/mobile/build/swift/static/xmtpv3.swift bindings/mobile/Sources/LibXMTP/
        cp LICENSE bindings/mobile/
        cd bindings/mobile && zip -r LibXMTPSwiftFFI.zip \
          Sources \
          build/swift/LibXMTPSwiftFFI.xcframework \
          LICENSE
    - compute checksum (shasum -a 256)
    - create/update GitHub release with zip
```

**Path changes from current workflow:**
- xcframework: `sdks/ios/.build/LibXMTPSwiftFFI.xcframework` -> `bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework`
- Swift source + headers: read from `build/nix/swift/` (Nix output symlink)
- No more downloading separate artifacts between jobs

**Unchanged jobs:** `compute-version`, `publish`

**Workflow becomes 3 jobs:** `compute-version` -> `build-and-package` -> `publish`

## Cachix

### What gets cached (per derivation)

- 4x `cargoArtifacts` (dep-only builds, change only when `Cargo.lock` changes)
- 4x per-target static library builds
- 1x Swift bindings (pure)
- 1x aggregate (trivial symlinks)

### Configuration

Already in place in `flake.nix`:
- `extra-substituters`: `https://xmtp.cachix.org`
- `extra-trusted-public-keys`: `xmtp.cachix.org-1:...`

CI needs `cachix/cachix-action` with push enabled. Developer machines are read-only consumers.

## Verification

### Nix flow (primary path)

1. **Derivation builds from clean state:**
   ```bash
   nix build .#ios-libs --out-link bindings/mobile/build/nix
   ```
   Verify all expected files exist in the output:
   - `bindings/mobile/build/nix/aarch64-apple-ios/libxmtpv3.a`
   - `bindings/mobile/build/nix/aarch64-apple-ios-sim/libxmtpv3.a`
   - `bindings/mobile/build/nix/x86_64-apple-darwin/libxmtpv3.a`
   - `bindings/mobile/build/nix/aarch64-apple-darwin/libxmtpv3.a`
   - `bindings/mobile/build/nix/swift/xmtpv3.swift`
   - `bindings/mobile/build/nix/swift/include/libxmtp/xmtpv3FFI.h`
   - `bindings/mobile/build/nix/swift/include/libxmtp/module.modulemap`

2. **xcframework assembly from Nix output:**
   ```bash
   nix develop .#ios --command make -C bindings/mobile lipo framework
   ```
   Verify `bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework` exists and contains all 3 platform slices (ios, ios-simulator, macos).

3. **Swift package builds against new xcframework path:**
   ```bash
   swift build
   ```
   Verify `Package.swift` finds the local xcframework at `bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework`.

4. **Full dev script end-to-end:**
   ```bash
   ./sdks/ios/dev/build
   ```
   Verify it completes successfully with no manual steps.

5. **Cache hit on second build:**
   ```bash
   nix build .#ios-libs --out-link bindings/mobile/build/nix
   ```
   Verify it completes near-instantly (no compilation).

### Pure cargo flow (Makefile without Nix)

6. **Makefile `local` target still works in the ios dev shell:**
   ```bash
   nix develop .#ios
   cd bindings/mobile
   make local
   ```
   Verify this still builds all 4 targets, generates Swift bindings, runs lipo, and produces the xcframework — all without depending on `build/nix/` existing. The `make local` target should continue to work as a self-contained build for developers who don't use `nix build`.

7. **Individual target build in dev shell:**
   ```bash
   nix develop .#ios
   cd bindings/mobile
   IPHONEOS_DEPLOYMENT_TARGET=14 cargo build --target aarch64-apple-ios-sim --release
   ```
   Verify single-target cargo builds still work for the inner dev loop.

8. **Swift tests link and run against the xcframework:**
   ```bash
   swift test --filter XMTPTests.ClientTests
   ```
   Verify a single test file passes, confirming the xcframework is correctly linked regardless of which flow produced it.
