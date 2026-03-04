# Mac Catalyst (maccatalyst) xcframework Slice

## Summary

This document describes how to add a Mac Catalyst (`ios-arm64_x86_64-maccatalyst`) slice
to the `LibXMTPSwiftFFI.xcframework`, enabling XMTP to be used in Mac Catalyst apps.

## Current State

The xcframework ships 3 slices:
- `ios-arm64` — iOS device (aarch64-apple-ios)
- `ios-arm64-simulator` — iOS Simulator on Apple Silicon (aarch64-apple-ios-sim)
- `macos-arm64_x86_64` — macOS universal (aarch64-apple-darwin + x86_64-apple-darwin)

## What's Needed

A fourth slice: `ios-arm64_x86_64-maccatalyst`, built from:
- `aarch64-apple-ios-macabi` — Mac Catalyst on Apple Silicon
- `x86_64-apple-ios-macabi` — Mac Catalyst on Intel

These are standard Rust targets available since Rust 1.77+ (tier 3) and present in
the project's pinned Rust 1.92.0 toolchain.

## Investigation Results

### ✅ Build Confirmed Working

Both targets compile successfully:
```bash
SDKROOT=$(xcrun --sdk macosx --show-sdk-path) \
  cargo build --target aarch64-apple-ios-macabi -p xmtpv3 --release

SDKROOT=$(xcrun --sdk macosx --show-sdk-path) \
  cargo build --target x86_64-apple-ios-macabi -p xmtpv3 --release
```

The resulting binaries have the correct `LC_BUILD_VERSION` metadata:
- platform = 6 (macCatalyst)
- minos = 14.0 (matches IPHONEOS_DEPLOYMENT_TARGET)

### ✅ xcframework Assembly Confirmed Working

`xcodebuild -create-xcframework` correctly produces the maccatalyst slice with:
- `SupportedPlatform: ios`
- `SupportedPlatformVariant: maccatalyst`
- `SupportedArchitectures: [arm64, x86_64]`

### ⚠️ Blocker: openssl-src Doesn't Know macabi Targets

The `openssl-src` crate (v300.5.4) has a target→OpenSSL config mapping that doesn't
include the macabi targets. The build fails with:

```
don't know how to configure OpenSSL for aarch64-apple-ios-macabi
```

**Fix:** Add two lines to `openssl-src/src/lib.rs`:
```rust
"aarch64-apple-ios-macabi" => "darwin64-arm64-cc",
"x86_64-apple-ios-macabi" => "darwin64-x86_64-cc",
```

This maps macabi to the same OpenSSL configs as native macOS, which is correct since
Mac Catalyst binaries use the macOS SDK and produce Mach-O binaries identical to macOS
(they just have platform=6 in LC_BUILD_VERSION instead of platform=1).

**Recommended approach:** Fork `openssl-src-rs` on GitHub, add the two lines, and
reference the fork via `[patch.crates-io]` in Cargo.toml:

```toml
[patch.crates-io]
openssl-src = { git = "https://github.com/xmtp/openssl-src-rs", branch = "macabi-support" }
```

Also upstream the fix to https://github.com/alexcrichton/openssl-src-rs.

### macabi Environment Setup

Mac Catalyst targets use the **macOS SDK** (not the iOS SDK). The environment setup:

- `SDKROOT` = macOS SDK path (`Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk`)
- `CC`/`CXX` = Xcode toolchain clang (same bypass as iOS targets to avoid Nix cc-wrapper)
- Target-specific bindgen args: `--target=arm64-apple-ios-macabi --sysroot=$SDKROOT`
  (or `--target=x86_64-apple-ios-macabi` for x86_64)
- Linker: Xcode toolchain clang

## Changes Required

### 1. Cargo.toml — openssl-src patch
Add `[patch.crates-io]` entry for openssl-src with macabi target support.

### 2. nix/lib/ios-env.nix
- Add `aarch64-apple-ios-macabi` and `x86_64-apple-ios-macabi` to `iosTargets`
- Add macabi entries to `isIosTarget` (they need CC/CXX bypass like iOS targets)
- Add macabi entries to `sdkSuffixForTarget` (point to MacOSX SDK)
- Add macabi-specific env var exports in `envSetup`
- Add macabi env vars to `envSetupAll`

### 3. nix/lib/mkToolchain.nix
No changes needed — it dynamically builds a toolchain from `iosTargets`.

### 4. bindings/mobile/Makefile
- Add `ARCHS_CATALYST = aarch64-apple-ios-macabi x86_64-apple-ios-macabi`
- Add build rules for catalyst targets (similar to iOS, using macOS SDK)
- Add `lipo` step for catalyst: `build/lipo_maccatalyst/libxmtpv3.a`
- Update `framework` target to include `-library build/lipo_maccatalyst/$(LIB)`
- Update `frameworkdyn` similarly for dynamic framework
- Update `.PHONY` targets
- Update `local` target to include catalyst targets

### 5. Package.swift (optional)
May need to add `.macCatalyst(.v14)` to platforms if SPM needs it for resolution.
The xcframework itself handles platform selection, so this may not be strictly needed.

### 6. CI (.github/workflows/release-ios.yml)
No changes needed — CI uses `nix build .#ios-libs` which reads from `iosTargets`.
Adding targets to `iosTargets` in ios-env.nix automatically includes them.
