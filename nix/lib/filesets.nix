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

  # Cargo path-dependency closures. A derivation that compiles only some
  # packages restores their closure over a dummy workspace, so an unrelated
  # crate, app, or document does not change its derivation.
  workspaceManifest = builtins.fromTOML (builtins.readFile (src + /Cargo.toml));
  workspaceDependencies = workspaceManifest.workspace.dependencies;
  patches = workspaceManifest.patch."crates-io";
  # Cargo resolves dev-dependencies only for the packages that a command selects.
  dependencyTables =
    dev: manifest:
    let
      kinds = [
        "dependencies"
        "build-dependencies"
      ]
      ++ lib.optional dev "dev-dependencies";
      tables = section: map (kind: section.${kind} or { }) kinds;
    in
    tables manifest ++ lib.concatMap tables (builtins.attrValues (manifest.target or { }));
  localDependencies =
    dev: path:
    lib.concatMap (
      table:
      lib.concatLists (
        lib.mapAttrsToList (
          name: value:
          let
            inherited = builtins.isAttrs value && (value.workspace or false);
            dependency = if inherited then workspaceDependencies.${name} else value;
            package = if builtins.isAttrs dependency then dependency.package or name else name;
            patch = patches.${package} or { };
            base = if inherited then src else path;
          in
          if builtins.isAttrs dependency && dependency ? path then
            [ { key = toString (base + "/${dependency.path}"); } ]
          else if patch ? path then
            [ { key = toString (src + "/${patch.path}"); } ]
          else
            [ ]
        ) table
      )
    ) (dependencyTables dev (builtins.fromTOML (builtins.readFile (path + "/Cargo.toml"))));
  asPath = key: src + (lib.removePrefix (toString src) key);
  # The package directories of the workspace `default-members`.
  defaultMembers = lib.concatMap (
    pattern:
    if lib.hasSuffix "/*" pattern then
      let
        parent = src + "/${lib.removeSuffix "/*" pattern}";
        entries = builtins.readDir parent;
      in
      map (name: parent + "/${name}") (
        builtins.filter (
          name: entries.${name} == "directory" && builtins.pathExists (parent + "/${name}/Cargo.toml")
        ) (builtins.attrNames entries)
      )
    else
      [ (src + "/${pattern}") ]
  ) workspaceManifest.workspace.default-members;
  # The package directories that `roots` compile. With `dev`, this includes the
  # roots' dev-dependencies, but not the dev-dependencies of other packages.
  closure =
    {
      roots,
      dev ? false,
    }:
    map (entry: asPath entry.key) (
      builtins.genericClosure {
        startSet =
          map (root: { key = toString root; }) roots
          ++ lib.optionals dev (lib.concatMap (localDependencies true) roots);
        operator = entry: localDependencies false (asPath entry.key);
      }
    );
  # Files outside Cargo sources and `.sql` files that a package reads when it
  # compiles. Keys are package directories relative to the repository root.
  compileData = {
    "apps/backend" = [
      (src + /apps/backend/migrations)
      (src + /apps/backend/.sqlx)
    ];
    "crates/xmtp_attachments" = [ (src + /crates/xmtp_attachments/src/address-registry.txt) ];
    "crates/xmtp_db" = [ (src + /crates/xmtp_db/migrations) ];
    "crates/xmtp_id" = [
      (src + /crates/xmtp_id/src/scw_verifier/chain_urls_default.json)
      (src + /crates/xmtp_id/src/scw_verifier/signature_validation.hex)
    ];
    "crates/xmtp_proto" = [ (src + /proto) ];
  };
  # Files that the `test-utils` feature compiles in. A dev-dependency can
  # enable it in any package of the closure.
  testUtilsData = {
    "crates/xmtp_id" = [ (src + /crates/xmtp_id/artifact) ];
  };
  # Files that only a package's own tests read.
  testData = {
    "apps/backend" = [
      (src + /docs/schemas/backend-v1.json)
      (src + /docs/backend-observability.md)
      (src + /docs/specs/OPS-backend-operations.md)
      (src + /dev/backend/local.toml)
      (src + /dev/backend/local-s3.toml)
    ];
    "crates/xmtp_mls_common" = [
      (src + /crates/xmtp_mls_common/src/mls_ext/payload_encryption/fixtures)
    ];
  };
  # The real sources of a closure, with its data and `extra` paths.
  closureSource =
    {
      roots,
      dev ? false,
      extra ? [ ],
    }:
    let
      crates = closure { inherit roots dev; };
      data =
        table: packages:
        lib.concatMap (
          package: table.${lib.removePrefix "${toString src}/" (toString package)} or [ ]
        ) packages;
    in
    lib.fileset.toSource {
      root = src;
      fileset = unions (
        [ (src + /Cargo.toml) ]
        ++ map commonCargoSources crates
        ++ map (crate: fileFilter (file: lib.hasSuffix ".sql" file.name) crate) crates
        ++ data compileData crates
        ++ lib.optionals dev (data testUtilsData crates ++ data testData roots)
        ++ extra
      );
    };
  workspaceSource = lib.fileset.toSource {
    root = src;
    fileset = workspace;
  };
  # Keep every other member as a stub for locked workspace resolution, and
  # restore the closure. `rust` is the Crane library of the caller's toolchain.
  mkClosureSource =
    rust: args:
    rust.mkDummySrc {
      src = workspaceSource;
      extraDummyScript = ''
        cp --recursive --remove-destination ${closureSource args}/. $out/
      '';
    };
in
{
  inherit
    depsOnly
    libraries
    binaries
    forCrate
    workspace
    defaultMembers
    closure
    closureSource
    mkClosureSource
    ;
}
