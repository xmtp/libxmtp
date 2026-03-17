{
  xmtp,
  lib,
  openssl,
  perl,
}:
let
  inherit (lib.fileset) unions;
  inherit (xmtp) craneLib;
  inherit (craneLib.fileset) commonCargoSources;

  commonArgs = {
    strictDeps = true;
    nativeBuildInputs = [
      openssl
      perl
    ];
  };

  rust-toolchain =
    p:
    xmtp.mkToolchain p
      [
        "x86_64-unknown-linux-musl"
        "aarch64-unknown-linux-musl"
      ]
      [ ];
  rust = xmtp.craneLib.overrideToolchain rust-toolchain;
  root = ./../..;
  # a full rebuild will be triggered only if these dependencies change.
  fileset = unions [
    (root + /Cargo.toml)
    (root + /Cargo.lock)
    (root + /apps/.gitkeep)
    (root + /bindings/.gitkeep)

    (root + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (root + /crates/xmtp_id/artifact)
    (root + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (root + /crates/xmtp_proto/src/gen/proto_descriptor.bin)

    (commonCargoSources (root + /crates/xmtp-workspace-hack))
    (commonCargoSources (root + /crates/xmtp_common))
    (commonCargoSources (root + /crates/xmtp_configuration))
    (commonCargoSources (root + /crates/xmtp_cryptography))
    (commonCargoSources (root + /crates/xmtp_id))
    (commonCargoSources (root + /crates/xmtp_proto))
    (commonCargoSources (root + /crates/xmtp_macro))
    (root + /apps/mls_validation_service/Cargo.toml)
    (root + /apps/mls_validation_service/build.rs)
  ];

  src = lib.fileset.toSource {
    inherit root fileset;
  };

  cargoArtifacts = rust.buildDepsOnly (
    commonArgs
    // {
      inherit src;
    }
  );
in
rust.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    pname = "mls-validation-service";
    cargoExtraArgs = "--bin mls-validation-service";
    version = xmtp.mkVersion rust;
    doCheck = false;
    src = lib.fileset.toSource {
      inherit root;
      fileset = unions [
        fileset
        (commonCargoSources (root + /apps/mls_validation_service))
      ];
    };
  }
)
