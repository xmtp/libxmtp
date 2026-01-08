{ lib
, craneLib
}:
let
  inherit (craneLib.fileset) commonCargoSources;
  libraries = lib.fileset.unions [
    ./../../Cargo.toml
    ./../../Cargo.lock
    (commonCargoSources ./../../crates/xmtp_api_grpc)
    (commonCargoSources ./../../crates/xmtp_cryptography)
    (commonCargoSources ./../../crates/xmtp_id)
    (commonCargoSources ./../../crates/xmtp_mls)
    (commonCargoSources ./../../crates/xmtp_api)
    (commonCargoSources ./../../crates/xmtp_api_d14n)
    (commonCargoSources ./../../crates/xmtp_proto)
    (commonCargoSources ./../../crates/xmtp_common)
    (commonCargoSources ./../../crates/xmtp_content_types)
    (commonCargoSources ./../../crates/xmtp_configuration)
    (commonCargoSources ./../../crates/xmtp_macro)
    (commonCargoSources ./../../crates/xmtp_db)
    (commonCargoSources ./../../crates/xmtp_db_test)
    (commonCargoSources ./../../crates/xmtp_archive)
    (commonCargoSources ./../../crates/xmtp_mls_common)
    (commonCargoSources ./../../crates/db_tools)
    ./../../crates/xmtp_id/src/scw_verifier/chain_urls_default.json
    ./../../crates/xmtp_id/artifact
    ./../../crates/xmtp_id/src/scw_verifier/signature_validation.hex
    ./../../crates/xmtp_db/migrations
    ./../../crates/xmtp_proto/src/gen/proto_descriptor.bin
    ./../../bindings/mobile/Makefile
    ./../../webdriver.json
    ./../../.config/nextest.toml
    ./../../.cargo/config.toml
  ];
  binaries = lib.fileset.unions [
    (commonCargoSources ./../../apps/cli)
    (commonCargoSources ./../../apps/mls_validation_service)
    (commonCargoSources ./../../bindings/node)
    (commonCargoSources ./../../bindings/wasm)
    (commonCargoSources ./../../crates/wasm_macros)
    (commonCargoSources ./../../bindings/mobile)
    (commonCargoSources ./../../crates/xmtp_debug)
  ];
  forCrate = crate: lib.fileset.unions [
    workspace
    crate
  ];
  workspace = lib.fileset.unions [
    binaries
    libraries
  ];
in
{
  inherit libraries binaries forCrate workspace;
}
