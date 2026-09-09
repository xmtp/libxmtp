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
    (fileFilter (file: file.name == "Cargo.toml" || file.name == "build.rs") (src + /bindings))
    apps
  ];

  libraries = unions (flatten [
    (src + /Cargo.toml)
    (src + /Cargo.lock)
    (src + /.cargo/config.toml)

    # include folders for apps/bindings so cargo workspace globs are satisfied
    # One-off files that are needed outside of cargo sources
    (src + /apps/.gitkeep)
    (src + /bindings/.gitkeep)
    (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
    (src + /crates/xmtp_id/artifact)
    (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    (src + /crates/xmtp_db/migrations)
    (lib.fileset.maybeMissing (src + /apps/backend/migrations))
    (lib.fileset.maybeMissing (src + /apps/backend/.sqlx))
    (src + /proto)
    (src + /webdriver.json)
    (lib.fileset.maybeMissing (src + /docs/schemas/backend-v1.json))
    (src + /dev/backend/local.toml)
    (src + /.config/nextest.toml)
    # all crates in `crates/` are treated as required library crates
    (crateSources (src + /crates))
    appSources
  ]);
  binaries = unions (flatten [
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
