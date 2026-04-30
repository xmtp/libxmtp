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
  inherit (lib.fileset) unions fileFilter;
  inherit (xmtp) craneLib base;
  inherit (craneLib.fileset) commonCargoSources;
  root = ./../..;

  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;

  # All workspace members' Cargo.toml/build.rs need to be present so
  # `cargo check --locked` evaluates the workspace without trying to
  # rewrite Cargo.lock to drop missing members.
  workspaceManifests = unions [
    (fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") (root + /apps))
    (fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") (root + /bindings))
  ];

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      workspaceManifests
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
