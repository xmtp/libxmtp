# Derivation that runs `cargo clippy -p xdbg`.
#
# xdbg is excluded from default-members in the root Cargo.toml, so the
# default per-PR Rust CI (clippy, nextest) skips it. This derivation is
# the per-PR gate that catches workspace changes which break xdbg's
# build before they land on main.
{
  xmtp,
  stdenv,
}:
let
  inherit (xmtp) craneLib base;
  root = ./../..;

  rust-toolchain =
    p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ "clippy-preview" ];
  rust = craneLib.overrideToolchain rust-toolchain;

  # Other members stay stubs so cargo --locked can resolve the workspace.
  src = xmtp.filesets.mkClosureSource rust {
    roots = [ (root + /apps/xmtp_debug) ];
    dev = true;
  };

  cargoArtifacts = base.mkCargoArtifacts rust false null;
in
rust.cargoClippy (
  base.commonArgs
  // {
    inherit src cargoArtifacts;
    cargoVendorDir = base.mkCargoVendorDir rust;
    pname = "xdbg";
    version = xmtp.mkVersion rust;
    cargoExtraArgs = "--locked --all-targets -p xdbg";
    cargoClippyExtraArgs = "--no-deps -- -Dwarnings";
    doCheck = false;
    doInstallCargoArtifacts = false;
  }
)
