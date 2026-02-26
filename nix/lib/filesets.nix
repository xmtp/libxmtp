{
  lib,
  xmtp,
}:
let
  inherit (xmtp.craneLib.fileset) commonCargoSources;
  inherit (lib.fileset) unions;
  inherit (lib.lists) flatten;
  src = ./../..;
  # List directores in a folder and apply `commonCargoSources`
  crateSources =
    cratesDir:
    let
      entries = builtins.readDir cratesDir;
      crateDirs = builtins.filter (name: entries.${name} == "directory") (builtins.attrNames entries);
    in
    map (name: commonCargoSources (cratesDir + "/${name}")) crateDirs;
  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  #
  # Used by both iOS and Android package derivations for consistent caching.
  depsOnly = unions [
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
  libraries = unions (flatten [
    (src + /Cargo.toml)
    (src + /Cargo.lock)
    # include folders for apps/bindings so cargo workspace globs are satisfied
    (src + /bindings)
    (src + /apps)
    # One-off files that are needed outside of cargo sources
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (src + /crates/xmtp_db/migrations)
    (src + /crates/xmtp_proto/src/gen/proto_descriptor.bin)
    (src + /webdriver.json)
    (src + /.cargo/config.toml)
    (src + /.config/nextest.toml)
    # all crates in `crates/` are treated as required library crates
    (crateSources (src + /crates))
  ]);
  binaries = unions (flatten [
    (src + /bindings/mobile/Makefile)
    (commonCargoSources (src + /apps/android/xmtpv3_example))
    (crateSources (src + /bindings))
    (crateSources (src + /apps))

  ]);
  forCrate =
    crate:
    let
      crates = if (builtins.isList crate) then crate else [ crate ];
    in
    lib.fileset.unions (
      [
        libraries
      ]
      ++ crates
    );
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
