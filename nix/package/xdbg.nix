# `xdbg` labels the compiled binary with the commit at run time.
#
# The compile (`passthru.unwrapped`, `.#xdbg.unwrapped`) reads no
# flake revision, so Cachix serves it across commits. Its store path changes
# with the xdbg source closure, and with any workspace manifest, `Cargo.lock`,
# or build script, because the dummy workspace keeps every manifest. The
# wrapper is the only part that reads `xmtp.gitSha`; it sets `XDBG_GIT_SHA`,
# which `get_version()` reads first.
{
  lib,
  xmtp,
  cacert,
  makeWrapper,
  runCommand,
  stdenv,
}:
let
  inherit (xmtp) base craneLib mkToolchain;
  rust-toolchain = p: mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;
  root = ./../..;
  version = xmtp.mkVersion rust;

  commonArgs = base.commonArgs // {
    nativeBuildInputs = base.commonArgs.nativeBuildInputs ++ [ cacert ];
  };
  # The default-members layer lacks the xdbg-only dependencies (`vergen-gix`,
  # `gix`, `clap-verbosity-flag`, `color-eyre`, `redb`), so build them here.
  cargoArtifacts = base.mkCargoArtifacts rust false {
    buildPhaseCargoCommand = "cargo build --locked --profile $CARGO_PROFILE -p xdbg";
  };

  unwrapped = rust.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts version;
      # Other members stay stubs so cargo --locked can resolve the workspace.
      src = xmtp.filesets.mkClosureSource rust {
        roots = [ (root + /apps/xmtp_debug) ];
      };
      cargoVendorDir = base.mkCargoVendorDir rust;
      # `build.rs` skips vergen when this is set. A constant keeps the
      # derivation the same across commits.
      NIX_GIT_SHA = "nix";
      pname = "xdbg";
      cargoExtraArgs = "--locked -p xdbg";
      meta.mainProgram = "xdbg";
    }
  );
in
runCommand "xdbg-${version}"
  {
    # `om ci` pins by `pname`, so keep the name of the old package.
    pname = "xdbg";
    inherit version;
    nativeBuildInputs = [ makeWrapper ];
    passthru = { inherit unwrapped; };
    meta.mainProgram = "xdbg";
  }
  ''
    # An unset or empty XDBG_GIT_SHA gets the flake revision. The revision is
    # hex, `<hex>-dirty`, or `unknown`, so it needs no quoting.
    makeWrapper ${lib.getExe unwrapped} $out/bin/xdbg \
      --run 'export XDBG_GIT_SHA=''${XDBG_GIT_SHA:-${xmtp.gitSha}}'
  ''
