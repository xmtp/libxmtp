{
  xmtp,
  lib,
  stdenv,
}:
let
  rust = xmtp.craneLib.overrideToolchain (
    p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ]
  );
  root = ./../..;
  src = lib.fileset.toSource {
    inherit root;
    # Keep workspace targets present for locked Cargo resolution.
    fileset = xmtp.filesets.workspace;
  };
  targetArgs = lib.optionalAttrs stdenv.hostPlatform.isMusl {
    RUSTFLAGS = "-C target-feature=+crt-static";
  };
  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
    targetArgs
    // {
      buildPhaseCargoCommand = "cargo build --locked --profile $CARGO_PROFILE -p xmtp_backend --bin xmtp-backend";
    }
  );
in
rust.buildPackage (
  xmtp.base.commonArgs
  // targetArgs
  // {
    inherit src cargoArtifacts;
    pname = "xmtp-backend";
    version = xmtp.mkVersion rust;
    cargoExtraArgs = "--locked -p xmtp_backend --bin xmtp-backend";
    doInstallCargoArtifacts = false;
    doCheck = false;
  }
)
