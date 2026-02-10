# Shared configuration for mobile (iOS/Android) cross-compilation derivations.
# Centralizes common build arguments, filesets, and version extraction.
#
# Usage:
#   mobile = import ./mobile-common.nix { inherit lib craneLib xmtp zstd openssl sqlite pkg-config perl zlib; };
#   # Then use: mobile.commonArgs, mobile.filesets, mobile.version, mobile.depsFileset, mobile.bindingsFileset
{ lib
, craneLib
, xmtp
, zstd
, openssl
, sqlite
, pkg-config
, perl
, zlib
}:
let
  # Shared filesets from nix/lib/filesets.nix
  filesets = xmtp.filesets { inherit lib craneLib; };

  # Narrow fileset for buildDepsOnly — only Cargo.toml, Cargo.lock, build.rs,
  # and files referenced by build scripts. Source (.rs) changes don't invalidate
  # the dep cache since crane replaces them with dummies anyway.
  depsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.depsOnly;
  };

  # Full fileset for buildPackage — includes all source files needed to compile
  # the xmtpv3 crate and its workspace dependencies.
  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.forCrate ./../../bindings/mobile;
  };

  # Common build arguments shared between iOS and Android derivations.
  # Platform-specific args (like ANDROID_HOME or __noChroot) are added by each derivation.
  commonArgs = {
    src = depsFileset;
    strictDeps = true;
    # perl is needed for openssl-sys's vendored build (its Configure script is Perl).
    nativeBuildInputs = [ pkg-config perl zlib ];
    buildInputs = [ zstd openssl sqlite ];
    doCheck = false;
    # Disable zerocallusedregs hardening which can cause issues with cross-compilation.
    hardeningDisable = [ "zerocallusedregs" ];
  };

in
{
  inherit filesets depsFileset bindingsFileset commonArgs;

  # Version extracted from workspace Cargo.toml — use this instead of calling
  # crateNameFromCargoToml multiple times in each derivation.
  # Note: This requires the caller to pass in a crane instance with the right toolchain.
  mkVersion = rust: (rust.crateNameFromCargoToml {
    cargoToml = ./../../Cargo.toml;
  }).version;
}
