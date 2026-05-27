{
  xmtp,
  lib,
  stdenv,
}:
let
  inherit (lib.fileset) unions;
  inherit (xmtp) craneLib mkToolchain base;
  inherit (craneLib.fileset) commonCargoSources;

  rust-toolchain = p: mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = xmtp.craneLib.overrideToolchain rust-toolchain;
  root = ./../..;

  specialArgs = lib.optionalAttrs stdenv.hostPlatform.isMusl {
    RUSTFLAGS = "-C target-feature=+crt-static";
  };

  commonArgs = base.commonArgs // specialArgs;

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      (root + /Cargo.toml)
      (root + /Cargo.lock)
      (root + /.cargo/config.toml)

      # Non-cargo files needed by build scripts
      (root + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
      (root + /crates/xmtp_id/artifact)
      (root + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
      (root + /crates/xmtp_proto/src/gen/proto_descriptor.bin)

      (root + /bindings/.gitkeep)
      (root + /apps/.gitkeep)
      (root + /apps/xnet/.gitkeep)

      # Actual source for crates needed to compile mls-validation-service
      (commonCargoSources (root + /crates/xmtp-workspace-hack))
      (commonCargoSources (root + /crates/xmtp_common))
      (commonCargoSources (root + /crates/xmtp_configuration))
      (commonCargoSources (root + /crates/xmtp_cryptography))
      (commonCargoSources (root + /crates/xmtp_id))
      (commonCargoSources (root + /crates/xmtp_proto))
      (commonCargoSources (root + /crates/xmtp_macro))
      (commonCargoSources (root + /apps/mls_validation_service))
    ];
  };

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false specialArgs;
in
rust.buildPackage (
  commonArgs
  // {
    inherit src cargoArtifacts;
    pname = "mls-validation-service";
    cargoExtraArgs = "--bin mls-validation-service";
    doInstallCargoArtifacts = false;
    version = xmtp.mkVersion rust;
    doCheck = false;
  }
)
