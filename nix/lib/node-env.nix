# Node.js cross-compilation environment configuration.
# Defines targets, name mapping, and cross-compilation helpers.
{
  lib,
  stdenv,
  pkgsCross,
}:
let
  # Targets are split by host-platform availability.
  # - gnu targets require glibc cross-compilation, which is broken on macOS
  #   (darwin-cross-build.patch fails to apply). Build these only on Linux.
  # - musl targets use self-contained musl toolchains that work everywhere.
  # - Darwin targets require Apple SDKs, so macOS only.
  # - Windows is excluded (built separately in CI).
  linuxGnuTargets = [
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
  ];

  linuxMuslTargets = [
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-musl"
  ];

  darwinTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
  ];

  nodeTargets = linuxMuslTargets ++ lib.optionals stdenv.isDarwin darwinTargets;

  # Rust triple -> NAPI-RS platform name (used in .node filenames).
  targetToNapi = {
    "x86_64-unknown-linux-gnu" = "linux-x64-gnu";
    "x86_64-unknown-linux-musl" = "linux-x64-musl";
    "aarch64-unknown-linux-gnu" = "linux-arm64-gnu";
    "aarch64-unknown-linux-musl" = "linux-arm64-musl";
    "x86_64-apple-darwin" = "darwin-x64";
    "aarch64-apple-darwin" = "darwin-arm64";
  };

  # Cross-compilation toolchains. Entries are needed for every target that isn't
  # the native host. Only musl targets need cross-compilers.
  crossCcFor = {
    "x86_64-unknown-linux-musl" = pkgsCross.musl64.stdenv.cc;
    "aarch64-unknown-linux-musl" = pkgsCross.aarch64-multiplatform-musl.stdenv.cc;
  };

  # Per-target CC, linker, and rustflags env vars for cargo cross-compilation.
  # Musl targets use -crt-static (required for cdylib) but statically link libc
  # via linker flags so the .node file is self-contained and works on glibc hosts.
  crossEnvFor =
    target:
    let
      cc = crossCcFor.${target} or null;
      targetUpper = builtins.replaceStrings [ "-" ] [ "_" ] (lib.toUpper target);
      isMusl = lib.hasInfix "musl" target;
    in
    (
      if cc == null then
        { }
      else
        let
          targetUnder = builtins.replaceStrings [ "-" ] [ "_" ] target;
        in
        {
          "CC_${targetUnder}" = "${cc.targetPrefix}cc";
          "AR_${targetUnder}" = "${cc.targetPrefix}ar";
          "RANLIB_${targetUnder}" = "${cc.targetPrefix}ranlib";
          "CARGO_TARGET_${targetUpper}_LINKER" = "${cc.targetPrefix}cc";
        }
    )
    // (
      if isMusl then
        {
          # -crt-static: allow cdylib (shared library) output on musl targets.
          # -nostdlib: prevent the GCC driver from adding its own -lc (dynamic).
          # Then we explicitly link static musl libc and libgcc so the .node file
          # is fully self-contained and works on any Linux host (glibc or musl).
          "CARGO_TARGET_${targetUpper}_RUSTFLAGS" = builtins.concatStringsSep " " [
            "-C target-feature=-crt-static"
            "-C link-arg=-nostdlib"
            "-C link-arg=-Wl,-Bstatic"
            "-C link-arg=-lc"
            "-C link-arg=-lgcc"
          ];
        }
      else
        { }
    );

  # Per-target nativeBuildInputs (cross-compilation toolchains).
  crossPkgsFor =
    target:
    let
      cc = crossCcFor.${target} or null;
    in
    if cc == null then [ ] else [ cc ];

  # Host Rust target triple for fast local builds and JS/TS generation.
  # Uses stdenv.hostPlatform.rust.rustcTarget which is the correct Rust triple.
  hostTarget = stdenv.hostPlatform.rust.rustcTarget;

in
{
  inherit
    nodeTargets
    targetToNapi
    hostTarget
    crossEnvFor
    crossPkgsFor
    ;
}
