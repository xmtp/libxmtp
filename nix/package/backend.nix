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
  workspaceSource = lib.fileset.toSource {
    inherit root;
    fileset = xmtp.filesets.workspace;
  };
  backendSource = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + /Cargo.toml)
      (rust.fileset.commonCargoSources (root + /apps/backend))
    ];
  };
  # The scaffold uses only std. Keep other workspace targets as stubs for
  # locked resolution; their Rust source does not affect the backend image.
  # Add dependency sources here when the backend starts using shared crates.
  src = rust.mkDummySrc {
    src = workspaceSource;
    extraDummyScript = ''
      cp --remove-destination ${backendSource}/Cargo.toml $out/Cargo.toml
      cp --recursive --remove-destination ${backendSource}/apps/backend/. $out/apps/backend/
    '';
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
