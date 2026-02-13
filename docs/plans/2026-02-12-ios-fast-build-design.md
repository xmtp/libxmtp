# iOS Fast Build Design

Build only the strictly necessary iOS targets for local development and CI, mirroring the Android `android-libs-fast` pattern.

## Problem

The iOS build compiles 4 Rust targets (aarch64-apple-ios, aarch64-apple-ios-sim, x86_64-apple-darwin, aarch64-apple-darwin) plus dynamic library variants, even for local dev where only simulator + host macOS are needed. Each target takes 30-60 minutes uncached.

## Design

### Fast targets (local dev / CI)

- `aarch64-apple-darwin` — host macOS (needed for `swift build`)
- `aarch64-apple-ios-sim` — iOS simulator

Static libraries only. No lipo (single arch per platform). 2-slice xcframework.

### Full targets (release)

All 4 targets, static + dynamic libraries, lipo'd fat binaries, 3-slice xcframework. Unchanged from today.

## Changes

### 1. `nix/package/ios.nix`

Add `mkIos` function that takes a target list and returns `{ targets, swiftBindings, aggregate }`. The existing top-level exports become `mkIos iosEnv.iosTargets` for backward compatibility. `swiftBindings` is shared (target-independent).

### 2. `flake.nix`

Add `ios-libs-fast` package:
```nix
ios-libs-fast = (pkgs.callPackage ./nix/package/ios.nix {
  stdenv = pkgs.stdenvNoCC;
}).mkIos [ "aarch64-apple-darwin" "aarch64-apple-ios-sim" ]).aggregate;
```

### 3. `bindings/mobile/Makefile`

Add `framework-fast` target: no lipo dependency, reads single-arch `.a` files directly from `$(NIX_OUT)/<target>/`, creates 2-slice xcframework (simulator + macOS), static only.

### 4. `sdks/ios/dev/bindings`

Default to fast build (`ios-libs-fast` + `framework-fast`). `--release` flag builds all targets (`ios-libs` + `lipo framework`).

## Validation

1. `./sdks/ios/dev/build` — fast path, `swift build` succeeds
2. `./sdks/ios/dev/build --release` — full path, `swift build` succeeds
3. `nix build .#ios-libs-fast` and `nix build .#ios-libs` both resolve
