{
  lib,
  xmtp,
}:
let
  inherit (xmtp.craneLib.fileset) commonCargoSources;
  inherit (lib.fileset) unions fileFilter;
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

  # Cargo resolves every workspace member before it applies default-members.
  apps = fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") (src + /apps);
  # Full app sources are required for Crane's dummy workspace. A manifest-only
  # app has no target when Cargo resolves the workspace in a Nix build.
  appSources = unions (crateSources (src + /apps));

  # Narrow fileset for buildDepsOnly — only includes files that affect
  # dependency compilation. Cargo.toml/Cargo.lock for resolution, build.rs
  # for build scripts, plus files referenced by build scripts.
  # Source (.rs) changes don't invalidate the dep cache since crane replaces
  # them with dummies anyway.
  #
  # Used by both iOS and Android package derivations for consistent caching.
  depsOnly = unions [
    (src + /Cargo.toml)
    (src + /Cargo.lock)
    (src + /.cargo/config.toml)
    (src + /proto)
    # All Cargo.toml and build.rs files in the workspace
    (fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") (src + /crates))
    apps
  ];

  libraries = unions (flatten [
    (src + /Cargo.toml)
    (src + /Cargo.lock)
    (src + /.cargo/config.toml)

    # One-off files that are needed outside of cargo sources
    (src + /apps/.gitkeep)
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    # Welcome compatibility tests read the stored keys and ciphertexts at compile time.
    (src + /crates/xmtp_mls_common/src/mls_ext/payload_encryption/fixtures)
    (src + /crates/xmtp_db/migrations)
    (src + /crates/xmtp_legacy_migration/migrations)
    (src + /crates/xmtp_legacy_migration/browser-storage.js)
    # The attachment client reads the IANA special-purpose address snapshot at compile time.
    (src + /crates/xmtp_attachments/src/address-registry.txt)
    (lib.fileset.maybeMissing (src + /apps/backend/migrations))
    (lib.fileset.maybeMissing (src + /apps/backend/.sqlx))
    (fileFilter (file: lib.hasSuffix ".sql" file.name) (src + /apps/backend/src))
    (src + /proto)
    (src + /webdriver.json)
    (lib.fileset.maybeMissing (src + /docs/schemas/backend-v1.json))
    # The backend metric catalogue test reads these to keep the docs in step.
    (lib.fileset.maybeMissing (src + /docs/backend-observability.md))
    (lib.fileset.maybeMissing (src + /docs/specs/OPS-backend-operations.md))
    (src + /dev/backend/local.toml)
    (src + /dev/backend/local-s3.toml)
    (src + /.config/nextest.toml)
    # all crates in `crates/` are treated as required library crates
    (crateSources (src + /crates))
    appSources
  ]);
  binaries = unions (flatten [
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
