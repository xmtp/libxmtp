# Node.js cross-compilation environment configuration.
# Defines targets, name mapping, and cross-compilation helpers.
{
  lib,
  stdenv,
  pkgsCross,
}:
let
  # All targets that Nix builds. Windows is excluded (built separately in CI).
  nodeTargets = [
    "x86_64-unknown-linux-gnu"
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-gnu"
    "aarch64-unknown-linux-musl"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
  ];

  # Rust triple -> NAPI-RS platform name (used in .node filenames).
  targetToNapi = {
    "x86_64-unknown-linux-gnu" = "linux-x64-gnu";
    "x86_64-unknown-linux-musl" = "linux-x64-musl";
    "aarch64-unknown-linux-gnu" = "linux-arm64-gnu";
    "aarch64-unknown-linux-musl" = "linux-arm64-musl";
    "x86_64-apple-darwin" = "darwin-x64";
    "aarch64-apple-darwin" = "darwin-arm64";
  };

  # Cross-compilation toolchains. Native targets (matching host) are absent.
  crossCcFor = {
    "aarch64-unknown-linux-gnu" = pkgsCross.aarch64-multiplatform.stdenv.cc;
    "aarch64-unknown-linux-musl" = pkgsCross.aarch64-multiplatform-musl.stdenv.cc;
    "x86_64-unknown-linux-musl" = pkgsCross.musl64.stdenv.cc;
  };

  # Per-target CC, linker, and rustflags env vars for cargo cross-compilation.
  # Musl targets need -crt-static to allow cdylib (.node shared library) builds.
  # This is set here (not .cargo/config.toml) to avoid conflicts with other musl
  # builds (e.g. musl-docker.nix) that need the opposite (+crt-static).
  crossEnvFor =
    target:
    let
      cc = crossCcFor.${target} or null;
      targetUpper = builtins.replaceStrings [ "-" ] [ "_" ] (lib.toUpper target);
      isMusl = lib.hasInfix "musl" target;
    in
    (if cc == null then
      { }
    else
      {
        "CC_${builtins.replaceStrings [ "-" ] [ "_" ] target}" = "${cc.targetPrefix}cc";
        "CARGO_TARGET_${targetUpper}_LINKER" = "${cc.targetPrefix}cc";
      })
    // (if isMusl then
      {
        "CARGO_TARGET_${targetUpper}_RUSTFLAGS" = "-C target-feature=-crt-static";
      }
    else
      { });

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
