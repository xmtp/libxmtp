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

  # Full app sources are required for Crane's dummy workspace. A manifest-only
  # app has no target when Cargo resolves the workspace in a Nix build.
  appSources = unions (crateSources (src + /apps));

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

  # Cargo path-dependency closures. A derivation that compiles only some
  # packages restores their closure over a dummy workspace, so an unrelated
  # crate, app, or document does not change its derivation.
  workspaceManifest = builtins.fromTOML (builtins.readFile (src + /Cargo.toml));
  workspaceDependencies = workspaceManifest.workspace.dependencies;
  patches = workspaceManifest.patch."crates-io";
  # The cfg values of the targets that a closure can be narrowed to. Key-value
  # names hold lists; bare names hold booleans. A `[target.*]` table that names
  # anything else fails evaluation, so a new cfg cannot drop a table silently.
  targetCfgs = {
    wasm32-unknown-unknown = {
      target_arch = [ "wasm32" ];
      target_endian = [ "little" ];
      target_env = [ "" ];
      target_family = [ "wasm" ];
      target_os = [ "unknown" ];
      target_pointer_width = [ "32" ];
      target_vendor = [ "unknown" ];
      unix = false;
      windows = false;
    };
  };
  # The tokens of a cfg expression: names, quoted strings, and `(`, `)`, `,`, `=`.
  cfgTokens =
    expression:
    builtins.filter (token: token != "") (
      map (part: if builtins.isList part then builtins.head part else lib.trim part) (
        builtins.split ''("[^"]*"|[(),=])'' expression
      )
    );
  # Evaluate the predicate that starts at token `i`: `all(…)`, `any(…)`,
  # `not(…)`, `name = "value"`, or `name`. Returns its value and the next index.
  cfgPredicate =
    cfg: tokens: i:
    let
      at = n: if n < builtins.length tokens then builtins.elemAt tokens n else null;
      name = at i;
      known = cfg ? ${name} || throw "filesets: unknown cfg name `${name}`; add it to targetCfgs";
    in
    if at (i + 1) == "(" then
      let
        list = cfgList cfg tokens (i + 2) [ ];
      in
      {
        value =
          if name == "all" then
            builtins.all lib.id list.values
          else if name == "any" then
            builtins.any lib.id list.values
          else if name == "not" && builtins.length list.values == 1 then
            !(builtins.head list.values)
          else
            throw "filesets: unsupported cfg operator `${name}`";
        inherit (list) next;
      }
    else if at (i + 1) == "=" then
      {
        value =
          assert known && builtins.isList cfg.${name};
          builtins.elem (lib.removePrefix "\"" (lib.removeSuffix "\"" (at (i + 2)))) cfg.${name};
        next = i + 3;
      }
    else
      {
        value =
          assert known && builtins.isBool cfg.${name};
          cfg.${name};
        next = i + 1;
      };
  # Evaluate a comma-separated predicate list up to its closing `)`.
  cfgList =
    cfg: tokens: i: values:
    if builtins.elemAt tokens i == ")" then
      {
        inherit values;
        next = i + 1;
      }
    else
      let
        predicate = cfgPredicate cfg tokens i;
        comma = builtins.elemAt tokens predicate.next == ",";
      in
      cfgList cfg tokens (if comma then predicate.next + 1 else predicate.next) (
        values ++ [ predicate.value ]
      );
  # Whether a `[target.<key>]` table applies to `target`: a `cfg(…)` key is
  # evaluated, and any other key is a target triple.
  targetApplies =
    target: key:
    let
      tokens = cfgTokens (lib.removeSuffix ")" (lib.removePrefix "cfg(" key));
      result = cfgPredicate targetCfgs.${target} tokens 0;
    in
    if lib.hasPrefix "cfg(" key && lib.hasSuffix ")" key then
      assert lib.assertMsg (
        result.next == builtins.length tokens
      ) "filesets: cannot parse the target key `${key}`";
      result.value
    else
      key == target;
  # Evaluation-time cases for `targetApplies`, one per key shape in the manifests.
  targetCases = [
    {
      key = ''cfg(all(target_family = "wasm", target_os = "unknown"))'';
      applies = true;
    }
    {
      key = ''cfg(not(all(target_family = "wasm", target_os = "unknown")))'';
      applies = false;
    }
    {
      key = ''cfg(not(any(target_os = "android", target_os = "ios", target_family = "wasm", target_os = "unknown")))'';
      applies = false;
    }
    {
      key = ''cfg(any(target_os = "android", target_os = "ios"))'';
      applies = false;
    }
    {
      key = ''cfg(target_arch = "wasm32")'';
      applies = true;
    }
    {
      key = ''cfg(not(target_arch = "wasm32"))'';
      applies = false;
    }
    {
      key = "cfg(unix)";
      applies = false;
    }
    {
      key = "cfg(all())";
      applies = true;
    }
    {
      key = "wasm32-unknown-unknown";
      applies = true;
    }
    {
      key = "x86_64-unknown-linux-gnu";
      applies = false;
    }
  ];
  checkedTargetApplies =
    assert lib.all (
      case:
      lib.assertMsg (
        targetApplies "wasm32-unknown-unknown" case.key == case.applies
      ) "filesets: `${case.key}` must give ${lib.boolToString case.applies} for wasm32"
    ) targetCases;
    targetApplies;
  # Cargo resolves dev-dependencies only for the packages that a command selects.
  # With a `target`, a `[target.*]` table that does not apply to it still adds its
  # build dependencies: the filter drops only what the target cannot use, and
  # build dependencies run on the build machine.
  dependencyTables =
    dev: target: manifest:
    let
      kinds = [
        "dependencies"
        "build-dependencies"
      ]
      ++ lib.optional dev "dev-dependencies";
      tables = kinds: section: map (kind: section.${kind} or { }) kinds;
    in
    tables kinds manifest
    ++ lib.concatLists (
      lib.mapAttrsToList (
        key: section:
        tables (
          if target == null || checkedTargetApplies target key then kinds else [ "build-dependencies" ]
        ) section
      ) (manifest.target or { })
    );
  localDependencies =
    dev: target: path:
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
    ) (dependencyTables dev target (builtins.fromTOML (builtins.readFile (path + "/Cargo.toml"))));
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
  # With `target` (a name in `targetCfgs`), dependencies that Cargo does not
  # build for that target stay out.
  closure =
    {
      roots,
      dev ? false,
      target ? null,
    }:
    map (entry: asPath entry.key) (
      builtins.genericClosure {
        startSet =
          map (root: { key = toString root; }) roots
          ++ lib.optionals dev (lib.concatMap (localDependencies true target) roots);
        operator = entry: localDependencies false target (asPath entry.key);
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
    "crates/xmtp_legacy_migration" = [
      (src + /crates/xmtp_legacy_migration/migrations)
      (src + /crates/xmtp_legacy_migration/browser-storage.js)
    ];
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
    "crates/xmtp_legacy_migration" = [ (src + /crates/xmtp_legacy_migration/fixtures) ];
    "crates/xmtp_mls" = [ (src + /crates/xmtp_legacy_migration/fixtures) ];
    "apps/backend" = [
      (src + /docs/schemas/backend-v1.json)
      (src + /docs/backend-observability.md)
      (src + /docs/specs/OPS-backend-operations.md)
      (src + /docs/specs/CONF-backend-configuration.md)
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
      target ? null,
      extra ? [ ],
    }:
    let
      crates = closure { inherit roots dev target; };
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
