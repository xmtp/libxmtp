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
    fileset = lib.fileset.unions (
      [
        (root + /Cargo.toml)
        (root + /proto)
        (lib.fileset.maybeMissing (root + /apps/backend/.sqlx))
        (lib.fileset.maybeMissing (root + /apps/backend/migrations))
        (lib.fileset.maybeMissing (root + /docs/schemas/backend-v1.json))
        (root + /dev/backend/config.toml)
        (rust.fileset.commonCargoSources (root + /apps/backend))
        (root + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
        (root + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
      ]
      ++ map (name: rust.fileset.commonCargoSources (root + "/crates/${name}")) [
        "xmtp_common"
        "xmtp_configuration"
        "xmtp_cryptography"
        "xmtp_id"
        "xmtp_logging"
        "xmtp_macro"
        "xmtp_mls_common"
        "xmtp_mls_validation"
        "xmtp_proto"
      ]
    );
  };
  # Keep unrelated clients as stubs for locked workspace resolution. Restore
  # the backend's shared dependency sources and embedded inputs only.
  src = rust.mkDummySrc {
    src = workspaceSource;
    extraDummyScript = ''
      cp --recursive --remove-destination ${backendSource}/. $out/
    '';
  };
  targetArgs = lib.optionalAttrs stdenv.hostPlatform.isMusl {
    RUSTFLAGS = "-C target-feature=+crt-static";
  };
  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
    targetArgs
    // {
      buildPhaseCargoCommand = "cargo build --locked --profile $CARGO_PROFILE -p xmtp_backend --bin xmtp-backend";
      SQLX_OFFLINE = "true";
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
    SQLX_OFFLINE = "true";
  }
)
