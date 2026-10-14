# `xdbg` labels the compiled binary with the commit at run time.
#
# The compile (`passthru.unwrapped`, `.#xdbg.unwrapped`) reads no
# flake revision, so Cachix serves it across commits. Its store path changes
# with the xdbg source closure, and with any workspace manifest, `Cargo.lock`,
# or `proto/` schema, because the dummy workspace keeps every manifest. The
# wrapper is the only part that reads `xmtp.gitSha`. It always sets
# `XDBG_GIT_SHA` to the build's commit, and `get_version()` reads the variable
# only in this placeholder build.
{
  lib,
  xmtp,
  cacert,
  makeWrapper,
  runCommand,
}:
let
  inherit (xmtp) base craneLib;
  rust = craneLib.overrideToolchain xmtp.hostToolchain;
  root = ./../..;
  version = xmtp.mkVersion rust;

  commonArgs = base.commonArgs // {
    nativeBuildInputs = base.commonArgs.nativeBuildInputs ++ [ cacert ];
  };
  cargoArtifacts = base.mkCargoArtifacts rust {
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
      # derivation the same across commits. `get_version()` in
      # `apps/xmtp_debug/src/main.rs` matches this placeholder.
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
    # The version keys stored state, so an inherited value must not override it.
    makeWrapper ${lib.getExe unwrapped} $out/bin/xdbg \
      --set XDBG_GIT_SHA ${lib.escapeShellArg xmtp.gitSha}
  ''
