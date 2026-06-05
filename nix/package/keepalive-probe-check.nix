# Derivation that runs `cargo clippy -p keepalive-probe`.
#
# keepalive-probe is excluded from default-members in the root Cargo.toml, so
# the default per-PR Rust CI (clippy, nextest) skips it. This derivation is
# the per-PR gate that catches workspace changes which break keepalive-probe's
# build before they land on main.
{
  xmtp,
  lib,
  stdenv,
}:
let
  inherit (xmtp) craneLib base;
  root = ./../..;

  rust-toolchain =
    p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ "clippy-preview" ];
  rust = craneLib.overrideToolchain rust-toolchain;

  # `workspace` covers every member so cargo --locked can resolve all manifests and targets.
  src = lib.fileset.toSource {
    inherit root;
    fileset = xmtp.filesets.workspace;
  };

  cargoArtifacts = base.mkCargoArtifacts rust false null;
in
rust.cargoClippy (
  base.commonArgs
  // {
    inherit src cargoArtifacts;
    pname = "keepalive-probe";
    version = xmtp.mkVersion rust;
    cargoExtraArgs = "--locked --all-targets -p keepalive-probe";
    cargoClippyExtraArgs = "--no-deps -- -Dwarnings";
    doCheck = false;
    doInstallCargoArtifacts = false;
  }
)
