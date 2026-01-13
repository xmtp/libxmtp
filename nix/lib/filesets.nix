{ lib
, xmtp
,
}:
let
  inherit (xmtp.craneLib.fileset) commonCargoSources;
  src = ./../..;

  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  #
  # Used by both iOS and Android package derivations for consistent caching.
  depsOnly = lib.fileset.unions [
    (src + /Cargo.lock)
    (src + /.cargo/config.toml)
    # All Cargo.toml and build.rs files in the workspace
    (lib.fileset.fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") src)
    # Files referenced by build scripts (e.g., include_bytes!, include_str!).
    # These are needed at dep-compilation time because build.rs runs then.
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (src + /crates/xmtp_db/migrations)
    (src + /crates/xmtp_proto/src/gen/proto_descriptor.bin)
  ];
  libraries = lib.fileset.unions [
    (src + /Cargo.toml)
    (src + /Cargo.lock)
    (src + /bindings)
    (src + /apps)
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (src + /crates/xmtp_db/migrations)
    (src + /crates/xmtp_proto/src/gen/proto_descriptor.bin)
    (src + /webdriver.json)
    (src + /.cargo/config.toml)
    (src + /.config/nextest.toml)
    (commonCargoSources (src + /crates/xmtp_api_grpc))
    (commonCargoSources (src + /crates/xmtp_cryptography))
    (commonCargoSources (src + /crates/xmtp_id))
    (commonCargoSources (src + /crates/xmtp_mls))
    (commonCargoSources (src + /crates/xmtp_api))
    (commonCargoSources (src + /crates/xmtp_api_d14n))
    (commonCargoSources (src + /crates/xmtp_proto))
    (commonCargoSources (src + /crates/xmtp_common))
    (commonCargoSources (src + /crates/xmtp_content_types))
    (commonCargoSources (src + /crates/xmtp_configuration))
    (commonCargoSources (src + /crates/xmtp_macro))
    (commonCargoSources (src + /crates/xmtp_db))
    (commonCargoSources (src + /crates/xmtp_db_test))
    (commonCargoSources (src + /crates/xmtp_archive))
    (commonCargoSources (src + /crates/xmtp_mls_common))
    (commonCargoSources (src + /crates/wasm_macros))
    (commonCargoSources (src + /crates/xmtp-workspace-hack))
  ];
  binaries = lib.fileset.unions [
    (src + /bindings/mobile/Makefile)
    (commonCargoSources (src + /apps/xnet/cli))
    (commonCargoSources (src + /apps/xnet/gui))
    (commonCargoSources (src + /apps/cli))
    (commonCargoSources (src + /apps/mls_validation_service))
    (commonCargoSources (src + /apps/android/xmtpv3_example))
    (commonCargoSources (src + /bindings/node))
    (commonCargoSources (src + /bindings/wasm))
    (commonCargoSources (src + /bindings/mobile))
    (commonCargoSources (src + /crates/xmtp_debug))
    (commonCargoSources (src + /crates/db_tools))
  ];
  forCrate =
    crate:
    lib.fileset.unions [
      libraries
      crate
    ];
  workspace = lib.fileset.unions [
    binaries
    libraries
  ];
in
{
  inherit
    depsOnly
    libraries
    binaries
    forCrate
    workspace
    ;
}
