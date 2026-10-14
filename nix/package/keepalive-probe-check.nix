# Derivation that runs `cargo clippy -p keepalive-probe`.
#
# keepalive-probe is excluded from default-members in the root Cargo.toml, so
# the default per-PR Rust CI (clippy, nextest) skips it. This derivation is
# the per-PR gate that catches workspace changes which break keepalive-probe's
# build before they land on main.
{
  xmtp,
}:
let
  inherit (xmtp) craneLib base;
  root = ./../..;

  rust = craneLib.overrideToolchain xmtp.hostToolchain;

  # Other members stay stubs so cargo --locked can resolve the workspace.
  src = xmtp.filesets.mkClosureSource rust {
    roots = [ (root + /apps/keepalive-probe) ];
    dev = true;
  };

  # Clippy reuses the check-mode metadata of third-party crates.
  cargoArtifacts = base.mkCargoArtifacts rust {
    buildPhaseCargoCommand = "cargo check --locked --profile $CARGO_PROFILE --all-targets -p keepalive-probe";
  };
in
rust.cargoClippy (
  base.commonArgs
  // {
    inherit src cargoArtifacts;
    cargoVendorDir = base.mkCargoVendorDir rust;
    pname = "keepalive-probe";
    version = xmtp.mkVersion rust;
    cargoExtraArgs = "--locked --all-targets -p keepalive-probe";
    cargoClippyExtraArgs = "--no-deps -- -Dwarnings";
    doCheck = false;
    doInstallCargoArtifacts = false;
    # A check result must name no store path, so it substitutes on a hit.
    allowedReferences = [ ];
  }
)
