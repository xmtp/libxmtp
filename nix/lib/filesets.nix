{ lib
, craneLib
}:
let
  inherit (craneLib.fileset) commonCargoSources;
  libraries = lib.fileset.unions [
    ./../../Cargo.toml
    ./../../Cargo.lock
    (commonCargoSources ./../../xmtp_api_grpc)
    (commonCargoSources ./../../xmtp_cryptography)
    (commonCargoSources ./../../xmtp_id)
    (commonCargoSources ./../../xmtp_mls)
    (commonCargoSources ./../../xmtp_api)
    (commonCargoSources ./../../xmtp_api_d14n)
    (commonCargoSources ./../../xmtp_proto)
    (commonCargoSources ./../../common)
    (commonCargoSources ./../../xmtp_content_types)
    (commonCargoSources ./../../xmtp_configuration)
    (commonCargoSources ./../../xmtp_macro)
    (commonCargoSources ./../../xmtp_db)
    (commonCargoSources ./../../xmtp_db_test)
    (commonCargoSources ./../../xmtp_archive)
    (commonCargoSources ./../../xmtp_mls_common)
    (commonCargoSources ./../../db_tools)
    ./../../xmtp_id/src/scw_verifier/chain_urls_default.json
    ./../../xmtp_id/artifact
    ./../../xmtp_id/src/scw_verifier/signature_validation.hex
    ./../../xmtp_db/migrations
    ./../../xmtp_proto/src/gen/proto_descriptor.bin
    ./../../bindings_ffi/Makefile
    ./../../webdriver.json
  ];
  binaries = lib.fileset.unions [
    (commonCargoSources ./../../examples/cli)
    (commonCargoSources ./../../mls_validation_service)
    (commonCargoSources ./../../bindings_node)
    (commonCargoSources ./../../bindings_wasm)
    (commonCargoSources ./../../bindings_wasm_macros)
    (commonCargoSources ./../../bindings_ffi)
    (commonCargoSources ./../../xmtp_debug)
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
