# Derivation that runs `cargo check -p xdbg`.
#
# xdbg is excluded from default-members in the root Cargo.toml, so the
# default per-PR Rust CI (clippy, nextest) skips it. This derivation is
# the per-PR gate that catches workspace changes which break xdbg's
# build before they land on main.
{
  xmtp,
  lib,
  stdenv,
}:
let
  inherit (lib.fileset) unions;
  inherit (xmtp) craneLib base;
  inherit (craneLib.fileset) commonCargoSources;
  root = ./../..;

  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      (commonCargoSources (root + /apps/xmtp_debug))
    ];
  };

  cargoArtifacts = base.mkCargoArtifacts rust false null;
in
rust.mkCargoDerivation (
  base.commonArgs
  // {
    inherit src cargoArtifacts;
    pname = "xdbg-check";
    version = xmtp.mkVersion rust;
    buildPhaseCargoCommand = "cargo check --locked --profile $CARGO_PROFILE -p xdbg";
    doCheck = false;
    doInstallCargoArtifacts = false;
  }
)
