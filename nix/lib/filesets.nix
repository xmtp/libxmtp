{ lib, craneLib, ... }:
let
  inherit (craneLib.fileset) commonCargoSources;
  filesetForCrate = crate: lib.fileset.toSource {
    root = ./../..;
    fileset = forCrate crate;
  };
  libraries = lib.fileset.unions [
    ./../../Cargo.toml
    ./../../Cargo.lock
    (commonCargoSources ./../../xmtp_api_grpc)
    (commonCargoSources ./../../xmtp_api_http)
    (commonCargoSources ./../../xmtp_cryptography)
    (commonCargoSources ./../../xmtp_id)
    (commonCargoSources ./../../xmtp_mls)
    (commonCargoSources ./../../xmtp_api)
    (commonCargoSources ./../../xmtp_api_d14n)
    (commonCargoSources ./../../xmtp_db)
    (commonCargoSources ./../../xmtp_proto)
    (commonCargoSources ./../../xmtp_db_test)
    (commonCargoSources ./../../common)
    (commonCargoSources ./../../xmtp_content_types)
    (commonCargoSources ./../../xmtp_macro)
    ./../../xmtp_id/src/scw_verifier/chain_urls_default.json
    ./../../xmtp_id/artifact
    ./../../xmtp_db/migrations
    ./../../.config
  ];
  binaries = lib.fileset.unions [
    (commonCargoSources ./../../examples/cli)
    (commonCargoSources ./../../mls_validation_service)
    (commonCargoSources ./../../bindings_node)
    (commonCargoSources ./../../bindings_wasm)
    (commonCargoSources ./../../bindings_ffi)
    (commonCargoSources ./../../xmtp_debug)
    (lib.fileset.maybeMissing ./../../bindings_ffi/libxmtp_version.txt)
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
  inherit libraries binaries forCrate workspace filesetForCrate;
}
